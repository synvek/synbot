//! TurboWorkflow: persistent, resumable multi-step workflows.
//!
//! Triggered by "twfw"/"turboworkflow" (case-insensitive) or intent "create workflow/twfw".
//! Workflow definition is JSON (LLM-generated or user-provided with confirmation).
//! State is persisted after each step so execution can resume after interrupt.

pub mod generator;
pub mod pending_confirm;
pub mod pending_input;
pub mod runner;
pub mod store;
pub mod trigger;
pub mod types;

pub use generator::generate_workflow;
pub use pending_confirm::PendingConfirmStore;
pub use pending_input::PendingWorkflowInputStore;
pub use runner::run_workflow;
pub use store::WorkflowStore;
pub use trigger::{parse_workflow_trigger, WorkflowTrigger};
pub use types::{WorkflowDef, WorkflowState, WorkflowStatus, WorkflowStepDef};
