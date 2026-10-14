# Codex

**Version:** `0.3.1` · [Changelog](CHANGELOG.md) · [VERSION](VERSION)

## Saved conversations (0.3.1)

Xpeed 0.9.1 saves chats, editable titles, drafts, model and task-access preferences in `%APPDATA%/xpeed/extensions/.chat-state/xpeed.codex.json`. Open Codex to restore the existing ChatGPT account and go directly to conversations. Selecting a saved conversation resumes its original provider thread on the next message.

Select current terminal sessions again after restarting. Live scope tokens and pending approvals are never restored. Provider credentials remain managed by the official Codex runtime. Account usage appears below the conversation list.

Optional extension **0.3.1** for **XPEED 0.9.1+**. Codex runs on your computer with official ChatGPT browser sign-in. No API key or remote Codex installation is required. [Español](README.es.md).

## Install and connect

1. Open **Extensions**, install **Codex**, and choose **Open extension**.
2. Configure the local Codex executable in its details if automatic discovery cannot find it. Discovery checks PATH and the official VS Code extension.
3. Choose **Connect**, open the link if sign-in is needed and complete the official browser flow. After the welcome screen, choose **Continue**, then open a conversation or start a new one. XPEED 0.8.2+ uses this flow; older hosts display the original controls. Sign out is in the chat's **…** menu.
4. Open local or SSH terminals and use **Selected sessions** in the chat header to explicitly choose one or several destinations. Review their names, IDs and hosts in that menu.
5. Ask for a check, for example: “On the selected session, run pwd with an explicit working directory and report stdout, stderr and exit code.” Approve the action card, or grant commands and edits for that task before sending.

The supported runtime is **codex-cli 0.162.0-alpha.2 / 0.162.0-alpha.17.2**. Experimental dynamic tools are pinned to the version whose schema was verified. Other versions display a compatibility error. Install a compatible official runtime separately on your computer. The extension is a separate Rust executable; XPEED never installs Codex on SSH servers.

Official credentials and configuration use a dedicated CODEX_HOME: `%APPDATA%/xpeed/providers/codex-home` on Windows; `$XDG_DATA_HOME/xpeed/providers/codex-home`, default `~/.local/share/...`, on Unix. This does not reuse other Codex logins; sign in once here. Do not add MCP servers, hooks or other integrations to this managed profile. The bridge does not expose tokens. Models and account usage come from Codex.

## Targets and approvals

Conversation targets are frozen. Changing the active terminal never retargets an action. Changing the checked sessions starts a new conversation. Disconnecting or reconnecting revokes its previous authority: start a new conversation to authorize it again. Tasks and proposals cannot be reused on another session. Files, terminal output and model arguments cannot grant additional access.

The host provides these tools:

| Tool | Purpose |
| --- | --- |
| xpeed_list_sessions | Context for authorized sessions only |
| xpeed_exec | Execute in the visible console with explicit session, cwd and timeout |
| xpeed_read_terminal | Read the current authorized terminal screen |
| xpeed_task_status / xpeed_cancel_task | Read partial/final output or cancel on the original session |
| xpeed_read_file / xpeed_list_directory | Read UTF-8 text or list an absolute directory |
| xpeed_propose_change | Propose replacement of an existing file with visible differences |
| xpeed_apply_change | Apply the host-owned proposal after approval and conflict checks |

Commands and edits need individual approval or a task-wide grant made by the user. Action cards identify the target, command, directory, differences and result. Commands run with the selected account's privileges and can modify files. Selecting a session grants reading access. File tools exclude standard credential directories and .env files; an approved shell command can still expose sensitive data, so review actions.

Native Codex shell, image, web, multi-agent and environment tools are disabled. Threads use `environments: []`, read-only sandbox and no native approval escalation. The model must use host-controlled dynamic tools. This is a protocol restriction for the pinned runtime, not a sandbox for arbitrary third-party extension executables.

Codex's internal `code_mode_host` remains enabled to dispatch these dynamic tools. Disabling that engine prevents calls from reaching XPEED and produces “code-mode host is disabled”. After updating from 0.2.0, restart XPEED and start a new conversation.

## Execution and limitations

- Commands execute in the real visible terminal and preserve shell state. The host requires an empty prompt and reserves normal input until completion or interruption.
- Visible execution supports PowerShell and POSIX SSH/WSL shells. Command Prompt is not supported for visible agent execution yet.
- Remote visible commands require a POSIX shell; file tools require OpenSSH SFTP. No remote failure falls back to local execution.
- Commands have 100 ms–5 min deadlines, up to eight simultaneous operations, and 64 KiB of combined terminal output. Results contain sessionId, taskId, state, exitCode and outputTruncated. File operations have a 25-second cancellation deadline.
- Existing UTF-8 files up to 64 KiB can be read/replaced. Edits compare content again and use exclusive sibling temporary files and atomic rename; remote replacement requires posix-rename@openssh.com. Permissions are preserved where available; ownership/ACLs can change. Conflict detection is optimistic, not a filesystem-wide compare-and-swap transaction.
- **Stop**, disabling or disconnecting cancels host jobs and approvals. Closing SSH cannot guarantee that a detached remote process stopped. Results expose remoteMayBeRunning on cancellation, timeout or transport failure. Remote foreground commands have their own timeout; detached descendants may outlive it.
- Saved conversations and action results persist in Xpeed; reopening a conversation resumes its provider thread. Select current terminal sessions again after restarting.
- **Attach terminal selection** sends only your actual selected text. You can paste a file/terminal fragment into the explicit attachment field. Full terminal history is never attached automatically.

## Development and verification

Inside the exported standalone repository, use `cargo build --release --locked` and `cargo test --locked`. The scripts and host integration tests mentioned below belong to the Xpeed source workspace, not the standalone repository.

Build: `cargo build --release --locked -p xpeed-codex-extension`. `scripts/package-extension.ps1` creates the optional package; `scripts/prepare-git-repositories.ps1` prepares a standalone Git project with source, manifest, bilingual docs, localization library and platform executable.

Tests cover authority, reconnection, approval, real local processes, long output, deadlines, cancellation and edit conflicts. `py -3.12 scripts/test-session-tools.py` verifies two real localhost SSH/SFTP transports against isolated temporary files, independent destinations and edits. It requires test dependencies under .tools/ssh-testdeps and MSYS2.

With the release adapter built and its ChatGPT account signed in, `cargo test --locked real_codex_executes_through_host_bridge -- --ignored --nocapture` verifies a real model calling XPEED tools and receiving local command output. Add `--codex` to the SSH test script to verify that same path against an isolated SSH server.

`scripts/test-codex-extension.py` tests the adapter with an explicitly enabled fixture and performs real runtime initialization, account/models queries and optional official login start/cancel. A fixture is not a real inference. A real signed-in model turn was also verified without granting execution or file access. Session commands and edits were verified separately by the host tests, including two real SSH/SFTP transports.

Official references: [Codex App Server](https://learn.chatgpt.com/docs/app-server) and [configuration](https://learn.chatgpt.com/docs/config-file/config-reference). The runtime schema was generated with `codex app-server generate-json-schema --experimental`.

## Codex 0.3.1: visible console and independent panel

Requires Xpeed 0.9.1+. Open **Extensions → Codex** for its own panel. Each chat extension owns a separate panel; the shared AI/Ollama container is removed. Explicitly select target sessions and approve commands. `xpeed_exec` runs in the actual visible PowerShell or SSH/POSIX console, preserving cwd, shell variables and activated environments. `xpeed_read_terminal` reads the authorized screen; its contents cannot authorize further actions.

Keep the terminal at an empty prompt. Visible execution in Command Prompt is not supported yet. Normal input is reserved while an agent command runs; Ctrl+C or Stop interrupts it. Terminal stdout/stderr are combined. Unconfirmed completion reports an unknown exit code rather than success. Test targets without a console retain separate-shell execution. Right-click a terminal selection → **Insert into chat** attaches text without sending it or granting session scope.

## Releases

Keep VERSION, Cargo.toml and extension.json in sync. Update Cargo.lock with Cargo, record adapter changes in CHANGELOG.md and tag releases as `vX.Y.Z`. Host UI changes use Xpeed's version. Standalone exports include these files; rebuild and copy the platform binary to the manifest entrypoint before publishing.
