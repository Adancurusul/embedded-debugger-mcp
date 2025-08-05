//! Flash programming and management

pub mod manager;

pub use manager::{
    FlashManager, 
    EraseType, 
    FileFormat, 
    EraseResult, 
    ProgramResult, 
    VerifyResult, 
    VerifyMismatch
};