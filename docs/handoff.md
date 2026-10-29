<!--
SPDX-FileCopyrightText: 2026 roolrz
SPDX-License-Identifier: Apache-2.0
-->

# Bootstrap handoff

Stage-1 and the formal root supervisor are independent userspace environments. This document
defines their supervision handoff and the ownership boundary for retirement.
The formal environment is not a numbered bootstrap stage. The kernel's boot contract
remains mounting the supplied archive at `/` and starting `/init` through the
delegated userspace loader; it does not interpret this protocol or its paths.
The shipped successor is component-manager. It validates the instance tree and
prepares system-startup to validate domain resources before accepting ownership.
Domain adoption runs in system-startup, outside the generic manager.

## Ownership phases

| Phase | Owner and permitted actions |
| --- | --- |
| Bootstrap | Stage-1 owns core Process supervisors and recovery policy. |
| Preparation | Stage-1 provisionally makes the independent system filesystem `/`, then starts a candidate component-manager. It validates its own policy and inventory, without core supervision authority. |
| Adoption | One capability-channel rendezvous moves all offered Process supervisors to component-manager. This is the irreversible supervision commit. |
| Operation | Component-manager commits the root, retains formal component supervisors and forwards domain resources to system-startup. Stage-1 init is parked; resident core services continue serving requests. |
| Retirement | System-startup submits bootstrap-retire after confirmed replacement. That job receives scoped predecessor authority, stops and joins the old processes, drains their console and requests unmount. |

Adoption does not authorize stage-1 to choose a retirement deadline, exit on
peer loss, or unmount the boot filesystem. Stage-1 does not run a second
supervisor after commit. The formal environment receives authority over the old
TaskGroup and owns both the decision and execution of its eventual retirement.

The candidate has a distinct TaskGroup. Formal services start in that group or its owned subgroups,
not in the inherited bootstrap group. Stopping the old group must not stop
system init or its newly launched services. Provider-created groups, such as
filesystem worker groups, must also be retired by their explicit owners.

## Candidate launch

Stage-1 selects an independently mounted filesystem and successor using its own
selected boot profile. Profiles live in `/etc/hyper/boot-profiles`; `default.json`
is mandatory. The kernel forwards arguments it does not recognize; stage-1 reads
`hyper.boot-profile=<name>` to select another installed profile. It does not
silently fall back when an explicitly requested profile is missing or invalid.
The profile's `handoff` section selects the root and successor:

```json
{"system-root":"/system","put-old":"/boot","system-init":"/svc/component-manager"}
```

Stage-1 gives the candidate a separate `BOOT_COMMAND_LINE` startup handle holding
the arguments it did not recognize. This uses the bounded, length-prefixed UTF-8
VMO format described in [Native startup](https://github.com/HypeR-Proj/kernel/blob/main/docs/design/native-init.md), with
`READ | INSPECT` rights. Reading consumes and closes the handle; the SDK returns
an owned string. It is not part of the supervision inventory and is not added to
component `argv` or environment variables. Stage-1 retains its string through
reversible launch attempts and builds a fresh immutable VMO for each candidate;
a failed attempt does not consume the next attempt's arguments. The current
component-manager reports any unrecognized remainder once and continues.

The system image contains existing `/boot`, `/tmp` and ordinary volume mountpoints.
Stage-1 begins a reversible root transition. The system filesystem becomes `/`;
the old root appears at `/boot`. After accepting supervision and committing the
transition, system-startup mounts a fresh RAMFS at `/tmp` before starting providers.
Read-only EROFS remains read-only: these are mount-table changes, not directory
writes. `/system` is only a bootstrap staging path and is not a formal path.
The system init and loader are opened after this switch, so every mapping refers
to the independent system mount rather than the archive.

The transition token retains both complete mount-table views. While pending,
concurrent mount, unmount and root-switch publications fail with `BUSY`. Ordinary
file access continues. Abort swaps back the original view without allocation or
backend I/O. It restores mount topology, not file contents written during the
provisional interval. Token owner loss also aborts. This mechanism supports mounted roots;
it does not promote or detach a RAM subtree. Stage-1 retains the token until the
supervision rendezvous transfers it atomically with the Process owners.
The candidate receives a read-only namespace root, a library directory,
diagnostic output, a description VMO and a receive endpoint. It also receives
preparation-only process creation, sponsorship, candidate-group attachment and
an executable loader. This permits component-manager to start system-startup in the same candidate
group. The coordinator validates the offered inventory, its finite migration
policy and realm exports, locks its mappings, then registers a private receive.
Only then does component-manager register the original ownership receive. On
preparation failure stage-1 stops and joins the entire candidate group. It receives no
core Process supervisors or device management capabilities during preparation.
The candidate explicitly locks its current mappings before declaring readiness.
Its builder is permitted to give its root VMAR `LOCK_MEMORY`; ordinary services
and the shell do not inherit that right. Failure to obtain the quota-charged
residency grant rejects preparation while the old providers are still running.

Private startup purposes are:

| Purpose | Object | Rights |
| --- | --- | --- |
| `0x80080001` | CapabilityChannel | `READ \| WAIT \| TRANSFER` |
| `0x80080002` | Immutable description VMO | `READ \| INSPECT \| TRANSFER` |

These are userspace protocol roles, not operation selectors or sender handle IDs.
The description contains a little-endian `u64` byte length followed by at most
32 KiB of UTF-8 JSON. Version 4 has these fields:

```json
{
  "version": 4,
  "root_mount": 7,
  "boot_root": "/boot",
  "services": [{"name": "io-runtime", "image": "/svc/io-runtime", "critical": true, "provider": "storage"}],
  "storage_successor": "/io-runtime",
  "storage_ready": true,
  "slots": [{"key": 1, "rights": 0}]
}
```

This example illustrates the shape, not a valid complete inventory. Each
service record supplies labels for an already launched process. Its nullable
`provider` field identifies `console`, `storage` or `filesystem` ownership; roles are
unique. The selected profile explicitly identifies the storage owner and its
formal successor; device-assignment authority alone does not identify an I/O VM.
Other roles come from typed boot capabilities rather than executable names. Stage-1's
dependency graph and capability-grant policy are not serialized. System-startup
loads its migration policy and binds replacement components to the offered
provider roles without relaunching their resources. `storage_ready` reports the
selected provider's bootstrap readiness result. It is true when no storage
readiness channel was requested; that case still requires an independent root
before handoff. It says nothing about later mounts or whether retirement is safe.

`storage_successor` is present exactly when a storage provider is offered.
It names a component instance, not an executable or process handle. Formal policy
must explicitly declare that instance as a resident, prepared `storage` provider
before accepting supervision. Several storage implementations may be declared;
only the selected one is started. An absent storage owner does not start the
I/O runtime. An offered owner without an admitted replacement rejects preparation.
This keeps plugin selection in stage-1 policy while formal policy retains control
over the implementations allowed to take ownership.

Unknown fields and versions, oversized offers, repeated or reserved slot keys,
unsupported object roles, and out-of-range service indices are rejected before
the candidate registers a receive operation. The two implementations need only
agree on this wire contract; neither links the other's supervisor library.

## Capability inventory

The slot vector gives the exact transfer order. Rights are Native right bits;
the receiver requires the advertised type and rights in every receive slot.
Required authority for namespace switching and planned formal services is
checked before adoption, including the duplication and transfer rights needed
to delegate each planned grant. Every critical core service must have a Process
supervisor slot. Each ready provider must also have its supervisor and private
migration endpoint; a failed optional storage provider may be absent when
`storage_ready` is false. An incomplete offer is rejected before READY rather
than failing after the irreversible MOVE.

| Key | Role | Object |
| --- | --- | --- |
| 1 | Live namespace root | Directory |
| 2 | Formal library directory | Directory |
| 3 | Task creation | TaskFactory |
| 4 | Bootstrap process group | TaskGroup |
| 5 | Resource sponsorship | ResourceDomain |
| 6–9 | Task, object, memory and CPU inspection, respectively | Corresponding inspector |
| 10 | Service log connector | CapabilityChannel |
| 11 | Unconsumed physical assignment authority | DeviceAssignmentAuthority |
| 12–14 | Raw console server, controller and log server, respectively | CapabilityChannel |
| 16–17 | I/O broker server and client, respectively | CapabilityChannel |
| 18 | Storage readiness | ByteChannel |
| 19–22 | Filesystem attach, provider, control server and control client, respectively | CapabilityChannel |
| 25 | VM creation | VirtualMachineCreationAuthority |
| 30 | Candidate/formal process group | TaskGroup |
| 31 | Diagnostic console | Console |
| 32 | Fixed bootstrap environment | Directory |
| 33 | Fixed system environment | Directory |
| 34 | Storage owner transfer endpoint | CapabilityChannel |
| 35 | Filesystem manager transfer endpoint | CapabilityChannel |
| 36 | Old console drain control | ByteChannel |
| 37 | Pending root transition | RootTransition |
| 100 + index | Existing core service supervisor | Process |

Optional capabilities already consumed by a service are absent. Keys 15, 23,
24 and 26–29 are reserved and rejected. Business VM fleet groups, budgets and
connection endpoints are created by system-startup and do not cross this boundary.
The transfer is bounded to 64 capabilities and the configured maximum service
count. Process keys refer to the corresponding description entry, not a PID.

The candidate registers the complete typed receive only after validation.
Stage-1 observes receiver readiness while continuing to watch core service and
candidate termination. It sends the exact bytes `HYPER-INIT-HANDOFF/4`, without
a terminator, alongside the capability inventory. Process supervisors and the root-transition token use MOVE;
management leases are duplicated so their active counts cannot transiently
reach zero. A successful rendezvous commits the whole transfer. A separate
acknowledgment is unnecessary and cannot turn committed adoption into rollback.
The predecessor then closes its copies of provider-handoff endpoints, the
console-drain endpoint and its retained service-log writer before parking.
Keeping those copies would hide successor death from providers or prevent the
old console from observing the final log EOF. Candidate launch temporaries,
including the local candidate-group lease, are also released; the committed
inventory already owns the successor group lease.

Preparation has a bounded timeout. Candidate failure, validation rejection or
timeout before commit stops only the candidate group and leaves bootstrap
recovery in charge. Stage-1 first stops the rejected candidate, aborts the root
transition and restores its retained bootstrap environment. Component-manager checks
that `root_mount` matches the candidate's namespace before registering receive.
After receiving the inventory, it commits the token before any service adoption
or mount request. A separate private rendezvous, `HYPER-SYSTEM-STARTUP/1`, moves
the remaining inventory to the already prepared coordinator. This message omits
the consumed root-transition token; all other slot order and rights are unchanged.
A failure after the first MOVE retains the known owners and reports a fatal error;
it cannot trigger stage1 rollback. Prepared formal processes are installed in the
manager using `HYPER-COMPONENT-INSTALL/1`; the coordinator receives WAIT observers,
while the manager retains their Process supervisors. After commit, candidate/system failure belongs to formal
supervision; stage-1 never resumes recovery or guesses that authority returned.

Recovery requires the stage-1 supervisor to remain alive. Its handoff buffers
use fallible reservations, and descriptor serialization rejects oversized
offers before allocating the encoded payload. A fatal supervisor fault, allocator
abort or external stop still closes its transition token and restores the pending
namespace, but does not recreate the supervisor or its service owners. This
protocol does not provide supervisor failover.

## Retirement requirements

Supervision transfer alone is insufficient to retire the boot filesystem.
The currently running boot providers still own resources and executable
mappings. System-startup must establish replacement owners before stopping them:

- A storage plugin must transfer all live resources needed by its successor.
  For the I/O VM implementation, the VM, vCPUs, physical assignments, mailbox identities and DMA leases
  must survive the old runtime. Hardware consumers quiesce at completed request
  boundaries; transferring ownership must not reset the VM or device.
- Filesystem replacements preserve the mounted transport, shared metadata buffer,
  logical block queue, read-only source-window backing and request sequence. They complete accepted requests and flush format state
  before transferring exclusive block access. Unmounting and remounting is not
  an equivalent handoff for open files or protected executable mappings.
- No formal interactive session exists before handoff. The new raw console
  obtains independent controller/log endpoints before formal services start.
  Old providers retain their log streams until replaced and stopped; the old
  raw worker drains those streams through EOF before bootstrap-retire stops it.
  A raw worker with no input sessions must not consume physical input.
- Files, fixed directories, cwd anchors, loader images and mappings referring
  to the old filesystem must be released before unmount can succeed.

A replacement filesystem provider must execute while its predecessor is
quiescent. Providers explicitly lock all current mappings, including existing
DSOs, before reporting READY or WAITING. Pages are charged against the process
resource domain's pinned-page limit; partial locking failure publishes no grant.
This does not create a FUTURE policy: newly loaded DSOs need a new explicit lock
before they are used on a self-paging path. Ordinary programs remain pageable.
The dynamic loader has no bootstrap residency environment-variable mode.

Formal supervision must distinguish stopping execution from completed teardown. A
termination signal alone does not prove that handles, address spaces, DMA or
boot backing have retired. Outstanding references defer retirement; they are
not bypassed through forced unmount or a guessed timeout. Errors and retries
after adoption remain formal-environment policy.

System-startup starts the formal raw console, adopts selected storage ownership, then adopts the
filesystem manager and workers before reading its formal fstab. Failed storage
adoption disables retirement and leaves failure handling with the formal environment.
Before resource transfer, original providers remain available. After transfer
starts, system-startup retains the known owners without claiming that a timeout returned
resources to their predecessors. Provider readiness alone never requests
process termination. System-startup runs the finite adoption sequence and submits
a separate `bootstrap-retire` job. Only exit zero after teardown and unmount
publishes the bootstrap-retired progress fact. Retirement follows formal mounts; business VM fleet readiness does not determine
whether storage resources have transferred.
Live core services without a provider-replacement protocol remain supervised
and defer the entire retirement attempt. The console, selected storage and
filesystem replacements do not implicitly replace arbitrary configured core
services. Only completed noncritical one-shots may be retired without a
replacement; critical exits remain subject to normal supervision.
The coordinator transfers predecessor supervisors and scoped namespace/group/drain
authority in one private rendezvous to the retirement oneshot, then releases its
fixed boot Directory. The oneshot stops predecessor providers, drains their old
console and joins the bootstrap TaskGroup before attempting unmount. The group
grant includes WAIT, INSPECT and REQUEST_STOP. It makes one bounded attempt. Failure is visible
as a failed component and startup-failed progress while healthy services remain
supervised. It cannot roll back a committed MOVE or completed stop. A busy mount
is never forcibly detached and there is no automatic retirement retry. See
[service management](https://hyper-proj.github.io/component-manager/docs/components.html) for policy and command details.

Interactive recovery is conditional on the system filesystem remaining usable.
The formal terminal is prepared after raw console and journal readiness, before
I/O and filesystem adoption. Component-manager reports startup progress and
retirement as complete lines alongside ordinary logs, without pausing raw output
or rewriting shared lines. The coordinator releases the first shell after
attempting retirement. If a post-commit failure loses the root filesystem
provider, the resident manager
can still print emergency diagnostics and supervise surviving owners, but it
may be unable to load the terminal or shell from that filesystem. This first
version does not promise an interactive recovery shell for that failure. It
does not restart the bootstrap fallback shell after supervision has committed.

The archive reservation is owned by the RAM filesystem nodes and borrowed
executable snapshots. Final owner release returns only full archive pages which
were not also reserved by firmware, the kernel or the DTB. With a separate system
volume, successful retirement detaches the archive; the `/boot` mountpoint may
remain as an empty directory.

The system filesystem must have independent backing before its init is launched.
An embedded system subtree sharing the bootstrap archive is not a production
layout: deleting `/boot` would not release archive bytes retained by file
mappings or remaining nodes. The kernel still supports RAMFS; an independently
constructed RAMFS can serve as the system root using the same handoff contract.

## Running provider transfer

Provider messages use independent versioned protocols. These startup purposes
are private to the two environments, with no kernel device-specific policy:

| Purpose | Role |
| --- | --- |
| `0x80080010` | Storage plugin resource handoff channel |
| `0x80080011` | Filesystem resource handoff channel |
| `0x80080013` | Filesystem description VMO |
| `0x80080014` | Predecessor raw-console drain channel |
| `0x80080020` | Replacement readiness writer |
| `0x80080021` | Old filesystem worker Process supervisors |

The storage migration channel is bidirectional and opaque to system-startup.
The coordinator forwards it to the selected formal replacement and waits for
`HYPER-SERVICE-ADOPTED/1` on the separate readiness writer. The predecessor and
replacement own their transfer protocol, including resource validation and the
commit boundary. A new native storage driver does not require teaching stage-1
or system-startup about its device state. It must keep serving before commit,
lock any mappings needed during transfer, and retain committed owners if
readiness reporting fails. The existing `NativeBlock` transport still uses guest
memory; this profile mechanism does not introduce a new native physical block
driver or change that kernel transport.

The I/O VM predecessor sends `HYPER-IO-DESCRIBE/2` and a length-prefixed, bounded
JSON description VMO (32 KiB maximum). The system replacement validates the
role/type/right inventory and ordinary guest-RAM bounds before receiving
`HYPER-IO-ADOPT/2`. The atomic MOVE includes the running VM and vCPUs, physical
device assignments, serial consumer and ring, ordinary guest-memory access,
an exclusive PCIe hotplug interrupt source, DMA memory leases and
Native/client mailboxes. The ordinary guest-memory capability admits only the
I/O VM's RAM; imported business-VM aliases are not device buffer authority.

The accompanying record preserves the serial cursor, partial-line state and
32-byte root-port register state. One MMIO consumer serves physical-device exits
and the empty PCIe slot during bootstrap. It joins at a request boundary before
MOVE, so no configuration update can occur between capture and adoption. Pending
vCPU exits stay in the kernel and are completed by the successor. A rejected
MOVE restarts the predecessor consumer with the same captured root-port state.
No physical device reset, Linux reboot or PCI host-bus re-enumeration occurs.

The formal runtime resumes that bus, creates its endpoint interrupt source and
may insert a virtio-console endpoint by publishing its configuration and a
standard PCIe presence-change interrupt.
Bootstrap provides an empty slot, without a console device or agent protocol.
The broker accepts business clients only in the formal owner.
`HYPER-SERVICE-ADOPTED/1` confirms the resumed owner is serving. This adoption readiness
record is separate from the unchanged bootstrap storage-readiness protocol.

The filesystem predecessor describes volume identities, labels and mounted
format/target state and kernel mount IDs with `HYPER-FS-DESCRIBE/1` (64 KiB maximum JSON). The formal
manager validates each mount ID against the live namespace, translating only
the exact current root mount to `/`. It prepares all resident workers before sending `HYPER-FS-PREPARED/1`.
Each old worker finishes accepted requests, synchronizes format state and
transfers its exclusive NativeBlock, unchanged server endpoint, shared metadata
VMO, logical block queue and request sequence. The new worker maps the same
queue-owned read-only source window, reopens that volume and resumes the same
transport; the kernel mount and open-file identities do not change. Directory
continuation cookies remain tied to media identity, not process-local handles.
Unserved volumes and the existing mount-administration endpoint transfer too.

Before PREPARED, failed candidates leave all original providers operational.
After resource transfer starts, a partial failure belongs to the formal environment; it
must not pretend all ownership returned to bootstrap. The new manager exports
old worker Process supervisors with `HYPER-BOOT-OWNER/1`, terminates that list
with `HYPER-BOOT-OWNERS-END/1`, then reports `HYPER-SERVICE-ADOPTED/1`. Old
workers and their manager remain parked until bootstrap-retire requests stop.

The predecessor console accepts `HYPER-CONSOLE-DRAIN/1` only without active
interactive sessions. It stops log admission, drains the ordered admission
fence and all existing writers through EOF, then replies
`HYPER-CONSOLE-DRAINED/1` and parks. It does not choose its own exit time.
