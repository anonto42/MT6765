## ADDED Requirements

### Requirement: Several terminals
The core SHALL start an additional instance of an app that declares `max_instances` greater than 1 when `new_instance` (or `launch` with `args.new`) is sent, up to that limit, and SHALL list every instance window in `windows`.

#### Scenario: a second terminal
- **WHEN** a terminal runs and `new_instance terminal` is sent
- **THEN** a second `st` process starts and `windows` lists two terminal windows with different ids

#### Scenario: the limit
- **WHEN** the instance limit is reached
- **THEN** `new_instance` fails with `refused`

#### Scenario: closing one
- **WHEN** `close_window` names one terminal window
- **THEN** only that window closes

### Requirement: Window modes
The core SHALL support the layout modes `single`, `split` and `free` and SHALL place windows inside the work area only.

#### Scenario: split in landscape
- **WHEN** the mode is split, the panel is 1280x800 and two apps are open
- **THEN** the two windows tile the work area side by side at the chosen ratio

#### Scenario: rotation
- **WHEN** the tablet rotates while split is on
- **THEN** the windows are tiled again for the new orientation (stacked in portrait)

#### Scenario: free windows stay inside
- **WHEN** a window is moved or resized beyond the work area or below the minimum size
- **THEN** the core clamps it to the work area and to at least 320x240

### Requirement: Struts are respected
The work area SHALL exclude the struts of the dock, the keyboard, the mouse pad and the window bar.

#### Scenario: keyboard up
- **WHEN** the system keyboard is shown in split mode
- **THEN** the tiles end above the keyboard
