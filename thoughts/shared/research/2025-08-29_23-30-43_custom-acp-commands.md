---
date: 2025-08-29 23:30:43 MDT
researcher: nathan
git_commit: 121345f0c62f5e924b4de67d6864cbba666c54d1
branch: acp-commands-nathan
repository: zed
topic: "Adding custom / commands to agent2 via ACP system from .agent/commands or .claude/commands directories"
tags: [research, codebase, agent2, acp, custom-commands]
status: complete
last_updated: 2025-08-29
last_updated_by: nathan
---

# Research: Adding custom / commands to agent2 via ACP system

**Date**: 2025-08-29 23:30:43 MDT
**Researcher**: nathan
**Git Commit**: 121345f0c62f5e924b4de67d6864cbba666c54d1
**Branch**: acp-commands-nathan
**Repository**: zed

## Research Question

How can we add custom / commands to the agent2 system that are read from `.agent/commands` or `.claude/commands` directories in the current worktree, using only the ACP (Agent Client Protocol) command system?

## Summary

Agent2 uses the ACP command system exclusively, which provides a session-scoped command framework via the `AgentSessionCommands` trait. The current implementation has command discovery working (`list()` method) but command execution is stubbed (`run()` method returns `Task::ready(Ok(()))`). This system is ideal for custom directory-based commands because it's session-scoped and has protocol support for dynamic command discovery.

**Key Finding**: The ACP command system already has the infrastructure for custom commands but needs the execution implementation completed.

## Detailed Findings

### ACP Command System Architecture

#### Core Components
- **Trait**: `AgentSessionCommands` in `/Users/nathan/src/zed/crates/acp_thread/src/connection.rs:107-110`
- **Implementation**: `AcpAgentSessionCommands` in `/Users/nathan/src/zed/crates/agent_servers/src/acp.rs:352-376`  
- **Protocol**: Agent Client Protocol with `list_commands` and `run_command` RPC methods

#### Current State Analysis

**Working Components:**
- Command discovery via `list()` method calls `connection.list_commands()` RPC
- Integration with completion system at `/Users/nathan/src/zed/crates/agent_ui/src/acp/completion_provider.rs:381-417`
- Command parsing via `parse_slash_command()` in `/Users/nathan/src/zed/crates/agent_ui/src/acp/message_editor.rs`
- Capability negotiation via `agent_capabilities.supports_commands`

**Missing Components:**
- **Critical**: `run()` method is stubbed at `/Users/nathan/src/zed/crates/agent_servers/src/acp.rs:373-375`
- No custom command loading mechanism
- No directory scanning for `.agent/commands` or `.claude/commands`

### Command Data Flow

#### Discovery Flow
1. User types `/` in message editor
2. Completion provider calls `commands.list(cx)` 
3. `AcpAgentSessionCommands.list()` calls `connection.list_commands()` RPC
4. Agent responds with `Vec<CommandInfo>`
5. Commands shown in autocomplete

#### Execution Flow (Currently Broken)
1. User selects command and presses enter
2. Command execution would call `commands.run(command, argument, cx)`
3. **STUB**: `AcpAgentSessionCommands.run()` returns `Task::ready(Ok(()))`
4. No actual command execution occurs

### Protocol Support

#### CommandInfo Structure
```rust
pub struct CommandInfo {
    pub name: String,           // Command identifier like "deploy"
    pub description: String,    // Human description like "Deploy to staging"  
    pub requires_argument: bool // Whether /deploy needs arguments like /deploy staging
}
```

#### RPC Methods
- `SESSION_LIST_COMMANDS` at `/Users/nathan/src/agent-client-protocol/rust/agent.rs:487` - **Working**
- `SESSION_RUN_COMMAND` - Protocol exists but **not used** in current implementation

### Integration Points

#### Command Provider Factory
```rust
// /Users/nathan/src/zed/crates/agent_servers/src/acp.rs:332-345
fn commands(&self, session_id: &SessionId, cx: &App) -> Option<Rc<dyn AgentSessionCommands>> {
    if self.agent_capabilities.supports_commands {
        Some(Rc::new(AcpAgentSessionCommands {
            session_id: session_id.clone(),
            connection: self.connection.clone(),
        }))
    } else {
        None
    }
}
```

#### Thread View Setup
```rust  
// /Users/nathan/src/zed/crates/agent_ui/src/acp/thread_view.rs:490-491
editor.set_command_provider(connection.commands(session_id, cx));
```

#### Parsing Logic
```rust
// /Users/nathan/src/zed/crates/agent_ui/src/acp/message_editor.rs
fn parse_slash_command(text: &str) -> Option<(usize, usize)> {
    if let Some(remainder) = text.strip_prefix('/') {
        let pos = remainder.find(char::is_whitespace).unwrap_or(remainder.len());
        let command = &remainder[..pos];
        if !command.is_empty() && command.chars().all(char::is_alphanumeric) {
            return Some((0, 1 + command.len()));
        }
    }
    None
}
```

### Capability System

#### Agent Capabilities  
Commands are conditional on `agent_capabilities.supports_commands` being true, negotiated during ACP initialization at `/Users/nathan/src/zed/crates/agent_servers/src/acp.rs:131-157`.

#### Session Scoping
Unlike traditional slash commands which are global, ACP commands are per-session, making them ideal for worktree-specific custom commands.

## Architecture Insights

### Design Patterns
1. **Capability-Driven**: Commands only available when agent advertises support
2. **Session-Scoped**: Each ACP session can have different available commands
3. **Protocol-Separated**: Command logic lives in agent, client handles UI/completion
4. **Async Task-Based**: All operations return `Task<Result<T>>` for GPUI integration

### Extension Strategy
The ACP system is designed for extensibility:
- `AgentSessionCommands` trait allows completely custom implementations
- Session-based discovery enables dynamic command loading per worktree
- Protocol abstraction allows commands from any source (agents, files, extensions)

## Custom Command Loading Strategy

### Key Extension Points
1. **AcpAgentSessionCommands Constructor**: Add custom command scanning
2. **list() Method**: Merge custom commands with agent-provided commands  
3. **run() Method**: Route execution between custom scripts and agent commands
4. **Command Provider Factory**: Pass worktree context for directory scanning

### Required Components
1. **Directory Scanner**: Scan `.agent/commands/` and `.claude/commands/` in worktree root
2. **Command Metadata Parser**: Extract name, description, argument requirements from files
3. **Script Executor**: Run custom command scripts with proper environment
4. **Command Router**: Determine whether to run custom script or forward to agent

### File Watching Opportunity
The session-scoped nature allows file watching to dynamically reload commands when directory contents change, providing live updates during development.

## Code References

- `crates/agent_servers/src/acp.rs:373-375` - **Stubbed run() method needing implementation**
- `crates/agent_servers/src/acp.rs:358-371` - Working list() method showing pattern
- `crates/agent_servers/src/acp.rs:332-345` - Command provider factory
- `crates/acp_thread/src/connection.rs:107-110` - AgentSessionCommands trait definition
- `crates/agent_ui/src/acp/completion_provider.rs:381-417` - Command completion integration
- `crates/agent_ui/src/acp/thread_view.rs:490-491` - Command provider setup
- `agent-client-protocol/rust/agent.rs:412-422` - CommandInfo structure definition

## Current Limitations

### Implementation Gaps
1. **No Command Execution**: `run()` method is completely stubbed
2. **No Custom Loading**: Only supports agent-provided commands
3. **No Worktree Context**: Command provider factory doesn't receive worktree path
4. **No File Watching**: Static command discovery only

### Protocol Completeness
The `SESSION_RUN_COMMAND` RPC method exists in the protocol but is not used by the current `AcpAgentSessionCommands` implementation.

## Historical Context

### Recent Changes
Your latest commit (121345f0c62f5e924b4de67d6864cbba666c54d1) shows the ACP command system was recently updated:
- Fixed capability access after ACP schema update
- `supports_commands` moved from `PromptCapabilities` to `AgentCapabilities`
- This indicates active development on the command system

### Agent2 vs Agent1
This research focuses exclusively on agent2's ACP command system, not the traditional slash command registry used elsewhere in Zed. The ACP system is designed specifically for agent interactions and session-scoped functionality.

## Next Research Areas

1. **Worktree Context**: How to pass worktree information through the command provider chain
2. **Script Execution**: Security models and execution environments for custom scripts  
3. **Metadata Formats**: What file formats should be supported for command definitions
4. **Error Handling**: How failures in custom commands should be reported to users
5. **Command Conflicts**: Resolution strategy when custom and agent commands have same names