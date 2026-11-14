## ADDED Requirements

### Requirement: Terminal sessions start an autosuggesting shell
The `terminal` app's `st` SHALL run `fish` as its command instead of the user's login shell, and
this SHALL be scoped to the `terminal` app's `exec` only (root's system login shell used by SSH
and other apps SHALL be unaffected).

#### Scenario: opening the terminal
- **WHEN** the `terminal` app is launched
- **THEN** the `st` process it starts runs `fish` and the prompt, autosuggestions and tab
  completion are `fish`'s own (no extra configuration shipped or required)

#### Scenario: SSH and the Kali profile are unaffected
- **WHEN** an SSH session is opened, or the `kali` app is launched
- **THEN** neither uses `fish`: SSH still runs the system shell (`/etc/passwd`), and `kali` still
  runs its own chroot shell via `/usr/local/sbin/walpad-kali shell`

### Requirement: No second terminal-tabs mechanism is introduced
A new change that touches the terminal app's windowing SHALL NOT introduce a second,
independent tab/session-switching UI for the terminal while the existing multi-instance window
bar (`multitasking-windows` change: `max_instances`, `new_instance`, `wmbar.rs`) already covers
opening and switching between terminal sessions.

#### Scenario: tabbed is not packaged, and conflicts with the existing window bar
- **WHEN** evaluating `tabbed` (suckless) as a terminal-tabs mechanism
- **THEN** it SHALL be rejected because Alpine packages no `tabbed` binary for any architecture,
  and because making it the terminal's top-level window would remove the embedded `st` sessions
  from `_NET_CLIENT_LIST`, breaking the window bar's existing per-window tabs, `new_instance` and
  the pid-based close-by-window-id behaviour for the `terminal` app
