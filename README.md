<!--
SPDX-FileCopyrightText: 2026 roolrz
SPDX-License-Identifier: Apache-2.0
-->

# lib-bootstrap-handoff

Formal-side bootstrap inventory and retirement protocol.

Defines handed-over service/process slots, committed inventory transfer and predecessor
retirement records. The component manager commits the root transition before the startup
coordinator moves domain resources.

See the [implementation entry](src/lib.rs) and [component declaration](component.json)
for the public interfaces, build dependencies and delivered artifacts.

## Boundaries

Stage1 does not link this library. Bootstrap retains its independent static protocol
implementation; sharing a wire format does not transfer lifecycle ownership or permit
early predecessor teardown.

## Build and test

Use this checkout inside the [repo-managed workspace](https://github.com/HypeR-Proj/HypeR).
From this repository directory:

```sh
make
make test
```

An absolute `OUT=/path/to/out` selects a shared output directory. The module
entry delegates to build/system, which prepares the installed SDK and declared
dependencies. Native binaries require HypeR; host module checks cover the tests
selected by component.json. A standalone clone does not contain the workspace’s
other source repositories or Native SDK.
