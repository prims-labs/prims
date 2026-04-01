//! Bibliothèque principale de Prims.
//!
//! Ce fichier expose les modules publics du prototype blockchain : réseau P2P,
//! structures blockchain, stockage, cryptographie, consensus, sharding,
//! confidentialité, API RPC et machine virtuelle Wasm.

pub mod network;

pub mod blockchain;

pub mod storage;

pub mod crypto;

pub mod consensus;

pub mod sharding;

pub mod privacy;

pub mod api;

pub mod vm;
