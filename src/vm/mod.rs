use crate::blockchain::types::{Account, FIXED_TRANSACTION_FEE, Transaction, TransactionType};
use crate::storage::RocksDbStorage;
use anyhow::{Result, anyhow, bail};
use std::{collections::BTreeMap, sync::Arc};
use wasmtime::{
    Caller, Config, Engine, Extern, Linker, Memory, Module, Result as WasmtimeResult, Store,
    ToWasmtimeResult,
};

pub const DEFAULT_WASM_FUEL_LIMIT: u64 = 100_000;

/// Configuration de gaz appliquée aux exécutions Wasm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmGasConfig {
    pub fuel_limit: u64,
}

impl Default for WasmGasConfig {
    fn default() -> Self {
        Self {
            fuel_limit: DEFAULT_WASM_FUEL_LIMIT,
        }
    }
}

/// Contexte d'exécution exposé à un contrat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmExecutionContext {
    pub contract_address: Vec<u8>,
    pub caller: Vec<u8>,
    pub block_height: u64,
    pub shard_id: u16,
}

/// Événement émis par un contrat pendant son exécution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractEvent {
    pub contract_address: Vec<u8>,
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub block_height: u64,
}

/// Machine virtuelle WASM minimale enrichie avec les host functions prévues par la roadmap.
#[derive(Debug, Default, Clone)]
pub struct WasmVM {
    context: WasmExecutionContext,
    pending_transfers: Vec<Transaction>,
    emitted_events: Vec<ContractEvent>,
    staged_storage_writes: BTreeMap<Vec<u8>, Vec<u8>>,
    gas_config: WasmGasConfig,
}

impl WasmVM {
    /// Construit une nouvelle VM WASM vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construit une VM WASM avec un contexte d'exécution déjà défini.
    pub fn with_context(context: WasmExecutionContext) -> Self {
        Self {
            context,
            pending_transfers: Vec::new(),
            emitted_events: Vec::new(),
            staged_storage_writes: BTreeMap::new(),
            gas_config: WasmGasConfig::default(),
        }
    }

    /// Retourne le contexte courant d'exécution.
    pub fn context(&self) -> &WasmExecutionContext {
        &self.context
    }

    /// Remplace le contexte courant d'exécution.
    pub fn set_context(&mut self, context: WasmExecutionContext) {
        self.context = context;
    }

    /// Retourne la configuration de gaz courante.
    pub fn gas_config(&self) -> WasmGasConfig {
        self.gas_config
    }

    /// Retourne la limite de fuel courante.
    pub fn fuel_limit(&self) -> u64 {
        self.gas_config.fuel_limit
    }

    /// Met à jour la limite de fuel courante.
    pub fn set_fuel_limit(&mut self, fuel_limit: u64) -> Result<()> {
        if fuel_limit == 0 {
            bail!("fuel limit must be greater than zero");
        }

        self.gas_config.fuel_limit = fuel_limit;
        Ok(())
    }

    fn metered_engine(&self) -> Result<Engine> {
        let mut config = Config::new();
        config.consume_fuel(true);
        Ok(Engine::new(&config)?)
    }

    /// Crée un store Wasmtime instrumenté pour consommer du fuel.
    pub fn new_metered_store<T: 'static>(&self, data: T) -> Result<Store<T>> {
        let engine = self.metered_engine()?;
        let mut store = Store::new(&engine, data);
        store.set_fuel(self.fuel_limit())?;
        Ok(store)
    }

    /// Retourne le fuel restant dans un store instrumenté.
    pub fn remaining_fuel<T: 'static>(&self, store: &Store<T>) -> Result<u64> {
        Ok(store.get_fuel()?)
    }

    fn require_contract_address(&self, operation: &str) -> Result<&[u8]> {
        if self.context.contract_address.is_empty() {
            bail!("contract address is required for {operation}");
        }

        Ok(&self.context.contract_address)
    }

    fn require_contract_account(
        &self,
        storage: &RocksDbStorage,
        operation: &str,
    ) -> Result<Account> {
        let contract_address = self.require_contract_address(operation)?;
        let account = storage
            .get_account(contract_address)?
            .ok_or_else(|| anyhow!("contract account does not exist"))?;

        if account.code_hash.is_none() {
            bail!("contract account is required for {operation}");
        }

        Ok(account)
    }

    /// Host function: lit le solde public d'une adresse.
    pub fn get_balance(&self, storage: &RocksDbStorage, address: &[u8]) -> Result<u64> {
        Ok(storage
            .get_account(address)?
            .map(|account| account.balance)
            .unwrap_or(0))
    }

    /// Host function: prépare un transfert public initié par le contrat.
    ///
    /// Pour cette étape, le transfert est seulement placé dans une file interne
    /// `pending_transfers` afin de définir proprement l'interface hôte sans encore
    /// brancher toute la pipeline d'exécution/validation de contrats.
    pub fn transfer(
        &mut self,
        storage: &RocksDbStorage,
        to: &[u8],
        amount: u64,
        data: Option<Vec<u8>>,
    ) -> Result<Transaction> {
        self.require_contract_address("transfer")?;

        if to.is_empty() {
            bail!("recipient address is required for transfer");
        }

        let sender = self.require_contract_account(storage, "transfer")?;

        let required = amount
            .checked_add(FIXED_TRANSACTION_FEE)
            .ok_or_else(|| anyhow!("transfer amount overflow"))?;

        if sender.balance < required {
            bail!(
                "contract balance is insufficient for transfer: balance={}, required={}",
                sender.balance,
                required
            );
        }

        let next_nonce = sender
            .nonce
            .checked_add(1)
            .ok_or_else(|| anyhow!("sender nonce overflow"))?;

        let transaction = Transaction {
            tx_type: TransactionType::Transfer,
            from: self.context.contract_address.clone(),
            to: to.to_vec(),
            amount,
            fee: FIXED_TRANSACTION_FEE,
            nonce: next_nonce,
            source_shard: self.context.shard_id,
            destination_shard: self.context.shard_id,
            signature: Vec::new(),
            data,
        };

        self.pending_transfers.push(transaction.clone());
        Ok(transaction)
    }

    /// Host function: retourne l'appelant courant.
    pub fn get_caller(&self) -> &[u8] {
        &self.context.caller
    }

    /// Host function: retourne la hauteur du bloc courant.
    pub fn get_block_height(&self) -> u64 {
        self.context.block_height
    }

    /// Host function: écrit dans l'espace de stockage du contrat.
    pub fn set_storage(
        &mut self,
        storage: &RocksDbStorage,
        key: &[u8],
        value: &[u8],
    ) -> Result<()> {
        self.require_contract_address("storage access")?;
        self.require_contract_account(storage, "storage access")?;

        self.staged_storage_writes
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    /// Host function: lit dans l'espace de stockage du contrat.
    pub fn get_storage(&self, storage: &RocksDbStorage, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let contract_address = self.require_contract_address("storage access")?.to_vec();
        self.require_contract_account(storage, "storage access")?;

        if let Some(value) = self.staged_storage_writes.get(key) {
            return Ok(Some(value.clone()));
        }

        storage.get_contract_storage(&contract_address, key)
    }

    /// Host function: enregistre un événement émis par le contrat.
    pub fn emit_event(&mut self, name: &[u8], data: &[u8]) -> Result<()> {
        self.require_contract_address("emit_event")?;

        if name.is_empty() {
            bail!("event name cannot be empty");
        }

        self.emitted_events.push(ContractEvent {
            contract_address: self.context.contract_address.clone(),
            name: name.to_vec(),
            data: data.to_vec(),
            block_height: self.context.block_height,
        });

        Ok(())
    }

    /// Retourne les transferts préparés par le contrat.
    pub fn pending_transfers(&self) -> &[Transaction] {
        &self.pending_transfers
    }

    /// Retourne les événements émis par le contrat.
    pub fn emitted_events(&self) -> &[ContractEvent] {
        &self.emitted_events
    }

    /// Vide et retourne les écritures de storage préparées.
    pub fn take_staged_storage_writes(&mut self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        std::mem::take(&mut self.staged_storage_writes)
    }

    /// Vide et retourne les transferts préparés.
    pub fn take_pending_transfers(&mut self) -> Vec<Transaction> {
        std::mem::take(&mut self.pending_transfers)
    }

    /// Vide et retourne les événements émis.
    pub fn take_emitted_events(&mut self) -> Vec<ContractEvent> {
        std::mem::take(&mut self.emitted_events)
    }
}

struct WasmHostState {
    storage: Arc<RocksDbStorage>,
    vm: WasmVM,
    params: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmExecutionOutcome {
    pub remaining_fuel: u64,
    pub pending_transfers: Vec<Transaction>,
    pub emitted_events: Vec<ContractEvent>,
}

impl WasmVM {
    fn memory_from_caller(caller: &mut Caller<'_, WasmHostState>) -> Result<Memory> {
        let export = caller
            .get_export("memory")
            .ok_or_else(|| anyhow!("contract memory export is required"))?;

        match export {
            Extern::Memory(memory) => Ok(memory),
            _ => bail!("contract memory export is required"),
        }
    }

    fn read_memory(caller: &mut Caller<'_, WasmHostState>, ptr: i32, len: i32) -> Result<Vec<u8>> {
        if ptr < 0 || len < 0 {
            bail!("negative memory offsets are not allowed");
        }

        let ptr = usize::try_from(ptr).map_err(|_| anyhow!("invalid memory pointer"))?;
        let len = usize::try_from(len).map_err(|_| anyhow!("invalid memory length"))?;
        let end = ptr
            .checked_add(len)
            .ok_or_else(|| anyhow!("memory range overflow"))?;

        let memory = Self::memory_from_caller(caller)?;
        let data = memory.data(caller);
        if end > data.len() {
            bail!("memory access is out of bounds");
        }

        Ok(data[ptr..end].to_vec())
    }

    fn write_memory(caller: &mut Caller<'_, WasmHostState>, ptr: i32, bytes: &[u8]) -> Result<()> {
        if ptr < 0 {
            bail!("negative memory offsets are not allowed");
        }

        let ptr = usize::try_from(ptr).map_err(|_| anyhow!("invalid memory pointer"))?;
        let end = ptr
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("memory range overflow"))?;

        let memory = Self::memory_from_caller(caller)?;
        let data = memory.data_mut(caller);
        if end > data.len() {
            bail!("memory write is out of bounds");
        }

        data[ptr..end].copy_from_slice(bytes);
        Ok(())
    }

    fn build_linker(&self, engine: &Engine) -> Result<Linker<WasmHostState>> {
        let mut linker = Linker::new(engine);

        linker.func_wrap(
            "prims",
            "get_block_height",
            |caller: Caller<'_, WasmHostState>| -> i64 {
                caller.data().vm.get_block_height() as i64
            },
        )?;

        linker.func_wrap(
            "prims",
            "get_caller_len",
            |caller: Caller<'_, WasmHostState>| -> WasmtimeResult<i32> {
                i32::try_from(caller.data().vm.get_caller().len())
                    .map_err(|_| anyhow!("caller length exceeds i32"))
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "copy_caller",
            |mut caller: Caller<'_, WasmHostState>, out_ptr: i32| -> WasmtimeResult<i32> {
                let bytes = caller.data().vm.get_caller().to_vec();
                Self::write_memory(&mut caller, out_ptr, &bytes).to_wasmtime_result()?;
                i32::try_from(bytes.len())
                    .map_err(|_| anyhow!("caller length exceeds i32"))
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "get_params_len",
            |caller: Caller<'_, WasmHostState>| -> WasmtimeResult<i32> {
                i32::try_from(caller.data().params.len())
                    .map_err(|_| anyhow!("params length exceeds i32"))
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "copy_params",
            |mut caller: Caller<'_, WasmHostState>, out_ptr: i32| -> WasmtimeResult<i32> {
                let params = caller.data().params.clone();
                Self::write_memory(&mut caller, out_ptr, &params).to_wasmtime_result()?;
                i32::try_from(params.len())
                    .map_err(|_| anyhow!("params length exceeds i32"))
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "get_balance",
            |mut caller: Caller<'_, WasmHostState>,
             address_ptr: i32,
             address_len: i32|
             -> WasmtimeResult<i64> {
                let address = Self::read_memory(&mut caller, address_ptr, address_len)
                    .to_wasmtime_result()?;
                let storage = Arc::clone(&caller.data().storage);
                let balance = caller
                    .data()
                    .vm
                    .get_balance(storage.as_ref(), &address)
                    .to_wasmtime_result()?;
                i64::try_from(balance)
                    .map_err(|_| anyhow!("balance exceeds i64"))
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "set_storage",
            |mut caller: Caller<'_, WasmHostState>,
             key_ptr: i32,
             key_len: i32,
             value_ptr: i32,
             value_len: i32|
             -> WasmtimeResult<()> {
                let key = Self::read_memory(&mut caller, key_ptr, key_len).to_wasmtime_result()?;
                let value =
                    Self::read_memory(&mut caller, value_ptr, value_len).to_wasmtime_result()?;
                let storage = Arc::clone(&caller.data().storage);
                caller
                    .data_mut()
                    .vm
                    .set_storage(storage.as_ref(), &key, &value)
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "get_storage_len",
            |mut caller: Caller<'_, WasmHostState>,
             key_ptr: i32,
             key_len: i32|
             -> WasmtimeResult<i32> {
                let key = Self::read_memory(&mut caller, key_ptr, key_len).to_wasmtime_result()?;
                let storage = Arc::clone(&caller.data().storage);
                match caller
                    .data()
                    .vm
                    .get_storage(storage.as_ref(), &key)
                    .to_wasmtime_result()?
                {
                    Some(value) => i32::try_from(value.len())
                        .map_err(|_| anyhow!("storage value length exceeds i32"))
                        .to_wasmtime_result(),
                    None => Ok(-1),
                }
            },
        )?;

        linker.func_wrap(
            "prims",
            "copy_storage",
            |mut caller: Caller<'_, WasmHostState>,
             key_ptr: i32,
             key_len: i32,
             out_ptr: i32|
             -> WasmtimeResult<i32> {
                let key = Self::read_memory(&mut caller, key_ptr, key_len).to_wasmtime_result()?;
                let storage = Arc::clone(&caller.data().storage);
                match caller
                    .data()
                    .vm
                    .get_storage(storage.as_ref(), &key)
                    .to_wasmtime_result()?
                {
                    Some(value) => {
                        let len = i32::try_from(value.len())
                            .map_err(|_| anyhow!("storage value length exceeds i32"))
                            .to_wasmtime_result()?;
                        Self::write_memory(&mut caller, out_ptr, &value).to_wasmtime_result()?;
                        Ok(len)
                    }
                    None => Ok(-1),
                }
            },
        )?;

        linker.func_wrap(
            "prims",
            "emit_event",
            |mut caller: Caller<'_, WasmHostState>,
             name_ptr: i32,
             name_len: i32,
             data_ptr: i32,
             data_len: i32|
             -> WasmtimeResult<()> {
                let name =
                    Self::read_memory(&mut caller, name_ptr, name_len).to_wasmtime_result()?;
                let data =
                    Self::read_memory(&mut caller, data_ptr, data_len).to_wasmtime_result()?;
                caller
                    .data_mut()
                    .vm
                    .emit_event(&name, &data)
                    .to_wasmtime_result()
            },
        )?;

        linker.func_wrap(
            "prims",
            "transfer",
            |mut caller: Caller<'_, WasmHostState>,
             to_ptr: i32,
             to_len: i32,
             amount: i64|
             -> WasmtimeResult<()> {
                if amount < 0 {
                    return Err(wasmtime::Error::msg("transfer amount must not be negative"));
                }

                let to = Self::read_memory(&mut caller, to_ptr, to_len).to_wasmtime_result()?;
                let storage = Arc::clone(&caller.data().storage);
                caller
                    .data_mut()
                    .vm
                    .transfer(storage.as_ref(), &to, amount as u64, None)
                    .to_wasmtime_result()?;
                Ok(())
            },
        )?;

        Ok(linker)
    }

    pub fn execute_contract_call(
        storage: Arc<RocksDbStorage>,
        context: WasmExecutionContext,
        method: &str,
        params: &[u8],
        fuel_limit: u64,
    ) -> Result<WasmExecutionOutcome> {
        if method.trim().is_empty() {
            bail!("call contract method must not be empty");
        }

        let contract = storage
            .get_contract(&context.contract_address)?
            .ok_or_else(|| anyhow!("contract does not exist"))?;

        let contract_address = context.contract_address.clone();

        let mut vm = WasmVM::with_context(context);
        vm.set_fuel_limit(fuel_limit)?;
        vm.require_contract_account(storage.as_ref(), "contract execution")?;

        let mut store = vm.new_metered_store(WasmHostState {
            storage: Arc::clone(&storage),
            vm: vm.clone(),
            params: params.to_vec(),
        })?;

        let module = Module::new(store.engine(), &contract.code_wasm)?;
        let linker = vm.build_linker(store.engine())?;
        let instance = linker.instantiate(&mut store, &module)?;
        let entry = instance
            .get_typed_func::<(), ()>(&mut store, method)
            .map_err(|err| anyhow!("failed to resolve exported function `{method}`: {err}"))?;

        entry.call(&mut store, ())?;

        let staged_storage_writes = store.data_mut().vm.take_staged_storage_writes();
        storage.commit_contract_storage_batch(&contract_address, &staged_storage_writes)?;

        let remaining_fuel = store.get_fuel()?;
        let pending_transfers = store.data_mut().vm.take_pending_transfers();
        let emitted_events = store.data_mut().vm.take_emitted_events();

        Ok(WasmExecutionOutcome {
            remaining_fuel,
            pending_transfers,
            emitted_events,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::{Account, AnonymousAccountState, Contract, DEFAULT_SHARD_ID};
    use crate::storage::{StorageBackend, keys};
    use std::{
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use wasmtime::{Instance, Module, Trap};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("prims-vm-{label}-{unique}"))
    }

    fn sample_account(balance: u64, nonce: u64) -> Account {
        Account {
            balance,
            nonce,
            code_hash: None,
            anonymous_state: AnonymousAccountState::default(),
        }
    }

    fn sample_contract_account(balance: u64, nonce: u64) -> Account {
        Account {
            balance,
            nonce,
            code_hash: Some(vec![7; 32]),
            anonymous_state: AnonymousAccountState::default(),
        }
    }

    fn sample_contract() -> Contract {
        Contract {
            code_wasm: b"\0asm\x01\0\0\0".to_vec(),
            storage_root: crate::blockchain::hash::sha256(&[]),
        }
    }

    fn sample_context() -> WasmExecutionContext {
        WasmExecutionContext {
            contract_address: b"contract-1".to_vec(),
            caller: b"caller-1".to_vec(),
            block_height: 42,
            shard_id: DEFAULT_SHARD_ID,
        }
    }

    fn execute_exported_run(wat: &str, fuel_limit: u64) -> Result<u64> {
        let mut vm = WasmVM::with_context(sample_context());
        vm.set_fuel_limit(fuel_limit)?;
        let mut store = vm.new_metered_store(())?;
        let module = Module::new(store.engine(), wat)?;
        let instance = Instance::new(&mut store, &module, &[])?;
        let run = instance.get_typed_func::<(), ()>(&mut store, "run")?;
        run.call(&mut store, ())?;
        vm.remaining_fuel(&store)
    }

    #[test]
    fn wasm_vm_new_starts_empty() {
        let vm = WasmVM::new();

        assert!(vm.context().contract_address.is_empty());
        assert!(vm.get_caller().is_empty());
        assert_eq!(vm.get_block_height(), 0);
        assert!(vm.pending_transfers().is_empty());
        assert!(vm.emitted_events().is_empty());
        assert_eq!(vm.fuel_limit(), DEFAULT_WASM_FUEL_LIMIT);
    }

    #[test]
    fn set_fuel_limit_rejects_zero() {
        let mut vm = WasmVM::new();
        let error = vm
            .set_fuel_limit(0)
            .expect_err("zero fuel limit should be rejected");

        assert!(
            error
                .to_string()
                .contains("fuel limit must be greater than zero")
        );
    }

    #[test]
    fn new_metered_store_starts_with_configured_fuel_limit() {
        let mut vm = WasmVM::new();
        vm.set_fuel_limit(1_234).expect("set fuel limit");

        let store = vm.new_metered_store(()).expect("create metered store");

        assert_eq!(vm.remaining_fuel(&store).expect("remaining fuel"), 1_234);
    }

    #[test]
    fn wasm_execution_consumes_fuel() {
        let wat = r#"
            (module
              (func (export "run")
                i32.const 1
                drop
                i32.const 2
                drop))
        "#;

        let remaining = execute_exported_run(wat, 50).expect("finite wasm should execute");

        assert!(remaining < 50, "fuel should be consumed during execution");
    }

    #[test]
    fn wasm_execution_traps_when_fuel_is_exhausted() {
        let wat = r#"
            (module
              (func (export "run")
                (loop
                  br 0)))
        "#;

        let mut vm = WasmVM::with_context(sample_context());
        vm.set_fuel_limit(10).expect("set small fuel limit");
        let mut store = vm.new_metered_store(()).expect("create metered store");
        let module = Module::new(store.engine(), wat).expect("compile loop module");
        let instance = Instance::new(&mut store, &module, &[]).expect("instantiate loop module");
        let run = instance
            .get_typed_func::<(), ()>(&mut store, "run")
            .expect("get run export");

        let error = run
            .call(&mut store, ())
            .expect_err("infinite loop should exhaust fuel");

        assert!(
            matches!(error.downcast_ref::<Trap>(), Some(trap) if *trap == Trap::OutOfFuel),
            "expected OutOfFuel trap, got: {error:?}"
        );
    }

    #[test]
    fn get_balance_reads_existing_and_missing_accounts() {
        let path = temp_path("get-balance");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        storage
            .update_account(b"alice", &sample_account(123, 7))
            .expect("save alice account");

        let vm = WasmVM::with_context(sample_context());

        assert_eq!(
            vm.get_balance(&storage, b"alice").expect("alice balance"),
            123
        );
        assert_eq!(vm.get_balance(&storage, b"bob").expect("bob balance"), 0);
    }

    #[test]
    fn transfer_queues_contract_transaction_when_balance_is_sufficient() {
        let path = temp_path("transfer-ok");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        storage
            .update_account(b"contract-1", &sample_contract_account(500, 9))
            .expect("save contract account");

        let mut vm = WasmVM::with_context(sample_context());
        let tx = vm
            .transfer(&storage, b"recipient-1", 25, Some(vec![1, 2, 3]))
            .expect("transfer should be prepared");

        assert_eq!(tx.tx_type, TransactionType::Transfer);
        assert_eq!(tx.from, b"contract-1".to_vec());
        assert_eq!(tx.to, b"recipient-1".to_vec());
        assert_eq!(tx.amount, 25);
        assert_eq!(tx.fee, FIXED_TRANSACTION_FEE);
        assert_eq!(tx.nonce, 10);
        assert_eq!(tx.source_shard, DEFAULT_SHARD_ID);
        assert_eq!(tx.destination_shard, DEFAULT_SHARD_ID);
        assert_eq!(tx.signature, Vec::<u8>::new());
        assert_eq!(tx.data, Some(vec![1, 2, 3]));
        assert_eq!(vm.pending_transfers(), &[tx]);
    }

    #[test]
    fn transfer_rejects_when_contract_balance_is_insufficient() {
        let path = temp_path("transfer-insufficient");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        storage
            .update_account(b"contract-1", &sample_contract_account(5, 0))
            .expect("save contract account");

        let mut vm = WasmVM::with_context(sample_context());
        let error = vm
            .transfer(&storage, b"recipient-1", 10, None)
            .expect_err("transfer should fail");

        assert!(
            error
                .to_string()
                .contains("contract balance is insufficient for transfer")
        );
    }

    #[test]
    fn set_storage_and_get_storage_roundtrip_under_contract_namespace() {
        let path = temp_path("storage-roundtrip");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        storage
            .update_account(b"contract-1", &sample_contract_account(0, 0))
            .expect("save contract account");
        storage
            .update_contract(b"contract-1", &sample_contract())
            .expect("save contract metadata");
        let mut vm = WasmVM::with_context(sample_context());

        vm.set_storage(&storage, b"counter", b"1")
            .expect("set storage");
        assert_eq!(
            vm.get_storage(&storage, b"counter").expect("get storage"),
            Some(b"1".to_vec())
        );
        assert_eq!(
            storage
                .get(&keys::contract_storage_key(b"contract-1", b"counter"))
                .expect("get raw storage"),
            None
        );

        let stored_contract = storage
            .get_contract(b"contract-1")
            .expect("get contract")
            .expect("contract should exist");
        let initial_contract = sample_contract();
        assert_eq!(stored_contract.code_wasm, initial_contract.code_wasm);
        assert_eq!(stored_contract.storage_root, initial_contract.storage_root);
    }

    #[test]
    fn set_storage_rejects_non_contract_accounts() {
        let path = temp_path("storage-non-contract");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        storage
            .update_account(b"plain-account", &sample_account(0, 0))
            .expect("save plain account");

        let mut context = sample_context();
        context.contract_address = b"plain-account".to_vec();
        let mut vm = WasmVM::with_context(context);

        let error = vm
            .set_storage(&storage, b"counter", b"1")
            .expect_err("storage access should fail");

        assert!(
            error
                .to_string()
                .contains("contract account is required for storage access")
        );
    }

    #[test]
    fn emit_event_records_contract_event_metadata() {
        let mut vm = WasmVM::with_context(sample_context());

        vm.emit_event(b"Transfer", br#"{"amount":25}"#)
            .expect("emit event");

        assert_eq!(vm.emitted_events().len(), 1);
        let event = &vm.emitted_events()[0];
        assert_eq!(event.contract_address, b"contract-1".to_vec());
        assert_eq!(event.name, b"Transfer".to_vec());
        assert_eq!(event.data, br#"{"amount":25}"#.to_vec());
        assert_eq!(event.block_height, 42);
    }

    #[test]
    fn execute_contract_call_runs_export_and_updates_storage() {
        let path = temp_path("execute-contract-call");
        let storage =
            Arc::new(RocksDbStorage::open(&path).expect("rocksdb should open successfully"));
        storage
            .update_account(b"contract-1", &sample_contract_account(0, 0))
            .expect("save contract account");

        let wat = r#"
            (module
              (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
              (import "prims" "emit_event" (func $emit_event (param i32 i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "counter")
              (data (i32.const 16) "1")
              (data (i32.const 32) "Incremented")
              (func (export "increment")
                i32.const 0
                i32.const 7
                i32.const 16
                i32.const 1
                call $set_storage
                i32.const 32
                i32.const 11
                i32.const 16
                i32.const 1
                call $emit_event))
        "#;

        let mut contract = sample_contract();
        contract.code_wasm = wat.as_bytes().to_vec();
        storage
            .update_contract(b"contract-1", &contract)
            .expect("save contract metadata");

        let outcome = WasmVM::execute_contract_call(
            Arc::clone(&storage),
            sample_context(),
            "increment",
            br#"{"delta":1}"#,
            50_000,
        )
        .expect("execute contract call");

        assert_eq!(
            storage
                .get_contract_storage(b"contract-1", b"counter")
                .expect("get contract storage"),
            Some(b"1".to_vec())
        );
        assert!(outcome.remaining_fuel < 50_000);
        assert!(outcome.pending_transfers.is_empty());
        assert_eq!(outcome.emitted_events.len(), 1);
        assert_eq!(outcome.emitted_events[0].name, b"Incremented".to_vec());
        assert_eq!(outcome.emitted_events[0].data, b"1".to_vec());
    }

    #[test]
    fn execute_contract_call_rolls_back_storage_on_trap() {
        let path = temp_path("execute-contract-call-trap");
        let storage =
            Arc::new(RocksDbStorage::open(&path).expect("rocksdb should open successfully"));
        storage
            .update_account(b"contract-1", &sample_contract_account(0, 0))
            .expect("save contract account");

        let wat = r#"
            (module
              (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "counter")
              (data (i32.const 16) "1")
              (func (export "increment_then_trap")
                i32.const 0
                i32.const 7
                i32.const 16
                i32.const 1
                call $set_storage
                unreachable))
        "#;

        let mut contract = sample_contract();
        contract.code_wasm = wat.as_bytes().to_vec();
        let initial_storage_root = contract.storage_root.clone();
        storage
            .update_contract(b"contract-1", &contract)
            .expect("save contract metadata");

        WasmVM::execute_contract_call(
            Arc::clone(&storage),
            sample_context(),
            "increment_then_trap",
            br#"{"delta":1}"#,
            50_000,
        )
        .expect_err("trap should abort contract execution");

        assert_eq!(
            storage
                .get_contract_storage(b"contract-1", b"counter")
                .expect("get contract storage after trap"),
            None
        );

        let stored_contract = storage
            .get_contract(b"contract-1")
            .expect("get contract")
            .expect("contract should exist");
        assert_eq!(stored_contract.storage_root, initial_storage_root);
    }

    #[test]
    fn execute_contract_call_rolls_back_storage_when_fuel_is_exhausted() {
        let path = temp_path("execute-contract-call-out-of-fuel");
        let storage =
            Arc::new(RocksDbStorage::open(&path).expect("rocksdb should open successfully"));
        storage
            .update_account(b"contract-1", &sample_contract_account(0, 0))
            .expect("save contract account");

        let wat = r#"
            (module
              (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "counter")
              (data (i32.const 16) "1")
              (func (export "increment_then_loop")
                i32.const 0
                i32.const 7
                i32.const 16
                i32.const 1
                call $set_storage
                (loop
                  br 0)))
        "#;

        let mut contract = sample_contract();
        contract.code_wasm = wat.as_bytes().to_vec();
        let initial_storage_root = contract.storage_root.clone();
        storage
            .update_contract(b"contract-1", &contract)
            .expect("save contract metadata");

        WasmVM::execute_contract_call(
            Arc::clone(&storage),
            sample_context(),
            "increment_then_loop",
            br#"{"delta":1}"#,
            10,
        )
        .expect_err("out of fuel should abort contract execution");

        assert_eq!(
            storage
                .get_contract_storage(b"contract-1", b"counter")
                .expect("get contract storage after out of fuel"),
            None
        );

        let stored_contract = storage
            .get_contract(b"contract-1")
            .expect("get contract")
            .expect("contract should exist");
        assert_eq!(stored_contract.storage_root, initial_storage_root);
    }

    #[test]
    fn execute_contract_call_rolls_back_storage_on_out_of_bounds_memory_access() {
        let path = temp_path("execute-contract-call-oob-memory");
        let storage =
            Arc::new(RocksDbStorage::open(&path).expect("rocksdb should open successfully"));
        storage
            .update_account(b"contract-1", &sample_contract_account(0, 0))
            .expect("save contract account");

        let wat = r#"
            (module
              (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "counter")
              (data (i32.const 16) "1")
              (func (export "increment_then_bad_memory")
                i32.const 0
                i32.const 7
                i32.const 16
                i32.const 1
                call $set_storage
                i32.const 0
                i32.const 7
                i32.const 65536
                i32.const 4
                call $set_storage))
        "#;

        let mut contract = sample_contract();
        contract.code_wasm = wat.as_bytes().to_vec();
        let initial_storage_root = contract.storage_root.clone();
        storage
            .update_contract(b"contract-1", &contract)
            .expect("save contract metadata");

        let error = WasmVM::execute_contract_call(
            Arc::clone(&storage),
            sample_context(),
            "increment_then_bad_memory",
            br#"{"delta":1}"#,
            50_000,
        )
        .expect_err("out-of-bounds memory access should abort contract execution");

        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("memory access is out of bounds")),
            "unexpected error chain: {error:#}"
        );

        assert_eq!(
            storage
                .get_contract_storage(b"contract-1", b"counter")
                .expect("get contract storage after invalid memory access"),
            None
        );

        let stored_contract = storage
            .get_contract(b"contract-1")
            .expect("get contract")
            .expect("contract should exist");
        assert_eq!(stored_contract.storage_root, initial_storage_root);
    }
}
