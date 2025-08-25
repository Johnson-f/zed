TODO dvdsk copy latest from notes

These are some terms and structures frequently used throughout the zed codebase. This is a best effort list. PR's are very welcome.

Questions:

- Can we generate this list form doc comments throughout zed?

## GPUI

- `Context`: A wrapper around the App struct with specialized behavior for a specific Entity
- `App`: A singleton which holds the full application state including all the entities.
- `AsyncApp`:
- `Window`:
- `Task`:
- `Executor`:
  - both background and foreground
- `Entity`: A strong, well-typed reference to a struct which is managed by gpui. Effectively a pointer into the Context.
- `Global`: A singleton type which has only one value
- `Event`: A datatype which can be send by an Entity to subscribers
- `Action`: An event that represents a user's keyboard input that can be handled by listerners
  Example: `file finder: toggle`
- `Subscription`: An event handler that is used to react to the changes of state in the application.
  1. Emitted event handling
  2. Observing {new,release,on notify} of an entity
- `Focus`: The place where keystrokes are handled first
- `Element`: An type that can be rendered
- `AnyElement`: A type erased version of an Element
- `element expression`: An expression that builds an element tree, example:
```
h_flex()
    .id(text[i])
    .relative()
    .when(selected, |this| {
        this.child(
            div()
                .h_4()
                .absolute()
                etc etc
```

## Zed

- `Workspace`: The root of the window <TODO add image>
- `Buffer`: The in memory representation of a 'file' together with relevant data such as syntax trees, git status and diagnostics.
- `Picker`: A model showing a list of items. When an item is selected you can confirm the item and then something happens. <TODO add image>
- `PickerDelegate`: A trait used to specialize behavior for a Picker
- `Modal`: todo! <TODO add image>
- `Project`: todo! <TODO add image>
- `Stores`: todo!

## Collab

- `Upstream client`: The zed client which has shared their workspace
- `Downstream client`: The zed client joining a shared workspace

## Debugger

- `DapStore`: Is an entity that manages debugger sessions
- `debugger::Session`: Is an entity that manages the lifecycle of a debug session and communication with DAPS
- `BreakpointStore`: Is an entity that manages breakpoints states in local and remote instances of Zed
- `DebugSession`: Manages a debug session's UI and running state
- `RunningState`: Directily manages all the views of a debug session
- `VariableList`: The variable and watch list view of a debug session
- `Console`
- `Terminal`
- `BreakpointList`
- `
