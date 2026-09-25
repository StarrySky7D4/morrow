# Windows context menu regression

## Coverage

| Surface | Context actions |
| --- | --- |
| Overview, inbox, project and experiment cards | View, edit, copy title/body, favorite, move inbox idea to project, delete with existing Undo, component settings |
| Navigation | Switch workspace page; component settings |
| Search | Clear search; component settings; text editing keeps its own native menu |
| Welcome/summary panels | Create idea; summary search reset; component settings |
| Quick capture | Submit the current note through its existing save path; component settings |
| Daily widget and individual items | Complete/reset tasks; individual completion; component settings |
| Music player | Import music, play/pause, previous/next, expand/collapse playlist, toggle footer lyrics, component settings |
| Playlist row | Play or remove the clicked track by object identity, independently of the current track |
| Versioned task row | Explicit completion, rename, copy, reorder and delete using existing stable TaskId commands |
| Footer | Toggle lyrics; component settings |

Menus support mouse secondary click, Shift+F10 and the keyboard menu key.
Opening/dismissing a menu does not execute a primary click. Plain surfaces can
receive keyboard focus. Disabled commands are checked again after menu selection;
task menus reject actions when their captured card view has been replaced.
Read-only/busy state and ambiguous legacy task decisions retain existing rules.
Material overrides continue to hide material-edit actions where appropriate.

## Windows build repair

Parallel MSVC plugin compilation reported C1041 for the shared debug PDB. The
Windows CMake configuration now enables `/FS` under MSVC to coordinate those
writes. A stale local cached workbench archive also failed the immutable-package
guard. It was retained under `build/context-menu-before/`; a fresh package was
verified against the existing Windows/Web local artifact before refreshing the
build cache. The matching SHA256 is
`ddf2bbb78039f74a67d3e81528e098298161931462748b26f7316dfaef1c262c`.
No plugin source/version change or user library migration was needed.

## Validation

- Targeted analysis of menu implementation and tests: no issues.
- Context workflow, task panel, UI style and music suites: passed.
- Desktop/mobile layout, search, navigation, appearance and persistence suite: passed.
- Real Windows Rust host integration: 3 passed (authorization, persistent content
  and preferences, protected-library recovery).
- Native Windows window integration: 18 passed, including context workflows,
  task ID targeting, stale/read-only rejection and correction after an unsent edit.
  The reused unit fixture now waits for the real input client/focus transition
  before replacing text after failed submission.
- Windows Release build passed. Executable:
  `build/windows/x64/runner/Release/morrow_studio.exe` (keep its adjacent files).
- Release self-check with a fresh, explicitly selected fixture library passed:
  desktop composition API, bundled silent-WAV decoder/playback clock, seeking,
  playback exclusion/no restore autoplay, and Rust-backed rendering without
  Flutter errors. `build/context-menu-release-check-path.txt` records the evidence
  directory containing `acceptance.md` and `acceptance.png`.

Evidence logs are in `build/context-menu-*.log`. Native window tests use isolated
fixture state and Flutter input events inside the real Windows runner; they are
not a claim of a physical mouse/keyboard or screen-reader hardware audit.
