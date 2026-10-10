# Structured Board Descriptions

Ariel OS introduces declarative, structured board descriptions.
These are intended to encode the information of what hardware is present on the board, in a way that is machine processable.
In Ariel OS, they are used to generate the `ariel-os-boards` package, that applications can use directly.

Hardware is diverse, and Ariel OS strives not to provide dedicated implementation for specific boards.
Instead, we describe the relevant properties of the board, and provide support for that property (even if, at some point in time, that property only applies to one board).

## SBD files

Structured board description (SBD) files are YAML files that follow the schema defined by the [`sbd-gen-schema` package][sbd-gen-schema-docsrs].
For each *target* of a board, they specify the chip, some OS-specific configuration required for the target, and what hardware is present on the board and is accessible and relevant for the target.
A board/SBD file may comprise multiple targets, e.g., when the board features multiple microcontrollers.
Targets correspond exactly to [Ariel OS's laze builders][laze-builders-book].
Usable chip names currently are Ariel OS's chip laze contexts.

> [!NOTE]
> The goal is that SBD files become completely OS-agnostic, however this is not entirely the case right now.

Existing SBD files are found in the [`boards/` directory][boards-dir-repo-main].
See [the documentation of `sbd_gen_schema`][sbd-gen-schema-docsrs] for the meaning of the various constructs.

Currently SBD files cannot be defined out of tree.
See [the Developer Guide][adding-support-board-book] to learn how to add support for a new board.
When adding a new SBD file, or updating an existing one, [`sbd-gen`][sbd-gen-cratesio] is used to re-generate the `ariel-os-boards` package from the SBD files.

## Using the Generated `ariel-os-boards` from Applications

To use the board information provided by `ariel-os-boards`, the package must be added as a dependency of the application, as already done in [the application templates][app-templates-book].

The types exposed by `ariel-os-boards`, generated from the SBD files, are documented in the relevant sections of this user guide.

[boards-dir-repo-main]: https://github.com/ariel-os/ariel-os/tree/main/boards
[sbd-gen-schema-docsrs]: https://docs.rs/sbd-gen-schema/latest/sbd_gen_schema/
[laze-builders-book]: ./build-system.md#laze-builders
[adding-support-board-book]: ./adding-board-support.md
[sbd-gen-cratesio]: https://crates.io/crates/sbd-gen
[app-templates-book]: ./getting-started.md#starting-an-application-project-from-a-template-repository
