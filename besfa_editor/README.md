# besfa_editor

Flutter desktop editor for Besfa.

The editor follows a lightweight Feature-Sliced Design layout:

- `app`: application bootstrap and app-wide configuration.
- `pages`: route-level screens that compose slices and widgets.
- `features`: user-facing behaviors, state, and actions.
- `widgets`: composed UI blocks for editor regions.
- `shared`: reusable UI and utilities with no feature ownership.
