//! Extism plugin ABI: export names and JSON shapes for host–plugin contract.

/// Tool: manifest (name, description, parameters_schema).
pub const FN_TOOL_MANIFEST: &str = "synbot_tool_manifest";
/// Tool: execute with args.
pub const FN_TOOL_CALL: &str = "synbot_tool_call";
/// Hook: receive lifecycle event.
pub const FN_HOOK_EVENT: &str = "synbot_hook_event";
/// Skills: list skill names.
pub const FN_SKILLS_LIST: &str = "synbot_skills_list";
/// Skills: load skill content by name.
pub const FN_SKILL_LOAD: &str = "synbot_skill_load";
/// Background: run (long-running until return or error).
pub const FN_BACKGROUND_RUN: &str = "synbot_background_run";
/// Provider: completion request → response.
pub const FN_COMPLETION: &str = "synbot_completion";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_constants_non_empty_and_prefixed() {
        let constants = [
            FN_TOOL_MANIFEST,
            FN_TOOL_CALL,
            FN_HOOK_EVENT,
            FN_SKILLS_LIST,
            FN_SKILL_LOAD,
            FN_BACKGROUND_RUN,
            FN_COMPLETION,
        ];
        for c in constants {
            assert!(!c.is_empty());
            assert!(c.starts_with("synbot_"), "{}", c);
        }
    }
}
