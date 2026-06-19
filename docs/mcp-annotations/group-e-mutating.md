# Group E — Mutating Tools: Annotation Accuracy Justification

This document justifies the MCP tool annotation hints declared in `src/tools/mod.rs`
for the **state-mutating** (write) tools of the Longbridge MCP server. Each annotation
value below was read from the actual `#[tool(..., annotations(...))]` attribute in
`mod.rs` and cross-checked against the implementation in the named module. All four
hints (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`) reflect
real observed behavior.

Annotation semantics used throughout:
- **readOnlyHint = false** — the tool changes environment / remote state.
- **destructiveHint = true** — the tool may delete, cancel, or overwrite existing data, or has an irreversible side effect; **= false** — the tool only appends or applies a reversible/toggle update.
- **idempotentHint = true** — repeating the same call with the same arguments produces no effect beyond the first; **= false** — repeating produces additional side effects (a second order, a duplicate row).
- **openWorldHint = true** — the tool interacts with an external system (the Longbridge brokerage / community / OpenAPI backend) rather than a closed local environment.

Every tool in this group sets `openWorldHint = true` because each one issues an
authenticated HTTPS request to a Longbridge backend (trade gateway, quote/watchlist
service, alert service, DCA service, community sharelist/topic service, or the OAuth
token endpoint). None operate on local-only state, so `openWorldHint = true` is
accurate in every section and is not re-argued individually below beyond noting the
specific backend touched.

---

## Section 1 — Create / append tools

Declared values: `read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true`.

### authenticate
Implementation: `src/tools/authenticate.rs::authenticate`.

- **Read Only = false** — The call exchanges a one-time authorization code for an access token and establishes an authenticated session; it mutates session/credential state and unlocks the full tool set, so it is not read-only.
- **Destructive = false** — It only establishes a new session (additive). It does not delete or overwrite any existing user data; an already-authenticated session is left untouched (the handler returns `already_authenticated` without altering credentials).
- **Idempotent = false** — The one-time code packed from the Connect AI page can be consumed only once. A second exchange of the same `auth_code` against the token endpoint fails (the code is spent), so repeating the same call does not reproduce the first call's success. Accurately non-idempotent.
- **Open World = true** — Performs an OAuth token exchange against the Longbridge authorization backend.

### create_watchlist_group
Implementation: `src/tools/quote.rs::create_watchlist_group` (`RequestCreateWatchlistGroup`).

- **Read Only = false** — Creates a new watchlist group (and optionally pre-populates securities) on the remote account.
- **Destructive = false** — Purely additive: it inserts a new group and never deletes or overwrites an existing one.
- **Idempotent = false** — Each call creates a brand-new group with its own server-assigned `id`. Calling twice with the same `name` yields two distinct groups, an additional side effect beyond the first call.
- **Open World = true** — Writes to the Longbridge watchlist service.

### alert_add
Implementation: `src/tools/alert.rs::alert_add`.

- **Read Only = false** — Registers a new price alert on the account.
- **Destructive = false** — Additive; it adds an alert and does not remove or overwrite any existing alert.
- **Idempotent = false** — Each call creates a new alert object (returned in the response). Repeating with the same condition produces a second, duplicate alert.
- **Open World = true** — Writes to the Longbridge alert service.

### topic_create
Implementation: `src/tools/content.rs::topic_create`.

- **Read Only = false** — Publishes a new community discussion topic (post or article).
- **Destructive = false** — Additive; it creates a new topic and does not modify or delete existing community content.
- **Idempotent = false** — Each call posts a new topic with its own id. Repeating creates duplicate posts.
- **Open World = true** — Writes to the Longbridge community backend.

### topic_create_reply
Implementation: `src/tools/content.rs::topic_create_reply`.

- **Read Only = false** — Posts a reply under an existing topic (optionally nested under another reply).
- **Destructive = false** — Additive; it appends a reply and does not alter or remove the parent topic or other replies.
- **Idempotent = false** — Each call posts a new reply; repeating produces duplicate replies.
- **Open World = true** — Writes to the Longbridge community backend.

### dca_create
Implementation: `src/tools/dca.rs::dca_create`.

- **Read Only = false** — Creates a recurring (DCA) investment plan on the account.
- **Destructive = false** — Additive; it creates a new plan and does not stop, overwrite, or delete any existing plan.
- **Idempotent = false** — Each call creates a distinct plan with its own `plan_id`. Repeating yields multiple recurring plans, an additional ongoing side effect.
- **Open World = true** — Writes to the Longbridge DCA/trade service.

### sharelist_create
Implementation: `src/tools/sharelist.rs::sharelist_create`.

- **Read Only = false** — Creates a new community sharelist (name + optional description).
- **Destructive = false** — Additive; it creates a new list and does not delete or overwrite existing lists.
- **Idempotent = false** — Each call returns a newly created sharelist with its own id; repeating creates duplicate lists.
- **Open World = true** — Writes to the Longbridge community sharelist service.

### sharelist_add
Implementation: `src/tools/sharelist.rs::sharelist_add` (`SharelistItemsParam`).

- **Read Only = false** — Adds securities to an existing community sharelist.
- **Destructive = false** — Additive; it appends symbols and does not remove or reorder existing constituents.
- **Idempotent = false** — The annotation declares non-idempotent: the tool sends an add operation each time and does not de-duplicate on the client side, so repeated calls are treated as repeated add side effects rather than a guaranteed no-op. `idempotent_hint = false` is the conservative, accurate hint here.
- **Open World = true** — Writes to the Longbridge community sharelist service.

---

## Section 2 — Destructive idempotent tools (delete / cancel / overwrite-update)

Declared values: `read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true`.

### delete_watchlist_group
Implementation: `src/tools/quote.rs::delete_watchlist_group` (`ctx.delete_watchlist_group(id, purge)`).

- **Read Only = false** — Deletes a watchlist group from the account.
- **Destructive = true** — It removes an existing group by id, and with `purge=true` also removes its securities from all other groups — irreversible data removal.
- **Idempotent = true** — After the group is deleted, re-issuing the delete for the same id has no additional effect (the object is already gone); the final state is identical.
- **Open World = true** — Mutates the Longbridge watchlist service.

### cancel_order
Implementation: `src/tools/trade.rs::cancel_order` (`ctx.cancel_order(order_id)`).

- **Read Only = false** — Cancels an open brokerage order.
- **Destructive = true** — Cancellation removes a pending order from the market — an irreversible state change to the order's lifecycle.
- **Idempotent = true** — Once an order is cancelled, the target final state (cancelled) is fixed; a repeated cancel of the same `order_id` adds no further effect (it errors / no-ops because the order is already cancelled or filled, per the description), leaving the same end state.
- **Open World = true** — Mutates state on the Longbridge trade gateway.

### alert_delete
Implementation: `src/tools/alert.rs::alert_delete`.

- **Read Only = false** — Deletes a price alert.
- **Destructive = true** — Removes an existing alert by id — irreversible removal.
- **Idempotent = true** — After deletion, re-deleting the same `alert_id` produces the same end state (alert absent), no extra effect.
- **Open World = true** — Mutates the Longbridge alert service.

### dca_stop
Implementation: `src/tools/dca.rs::dca_stop`.

- **Read Only = false** — Permanently stops a DCA plan.
- **Destructive = true** — The description states this "cannot be undone"; it terminates the plan irreversibly (distinct from the reversible `dca_pause`).
- **Idempotent = true** — Once stopped, repeating the stop on the same `plan_id` leaves the plan in the same terminal stopped state with no additional effect.
- **Open World = true** — Mutates the Longbridge DCA service.

### sharelist_delete
Implementation: `src/tools/sharelist.rs::sharelist_delete`.

- **Read Only = false** — Deletes a community sharelist (own lists only).
- **Destructive = true** — Removes an existing list — irreversible removal of the list and its membership.
- **Idempotent = true** — After deletion, re-issuing for the same id yields the same end state (list absent).
- **Open World = true** — Mutates the Longbridge community sharelist service.

### sharelist_remove
Implementation: `src/tools/sharelist.rs::sharelist_remove` (`SharelistItemsParam`).

- **Read Only = false** — Removes securities from a community sharelist.
- **Destructive = true** — Removes constituents from an existing list — a removal/deletion operation on existing data.
- **Idempotent = true** — Removing the same symbols again converges to the same end state (those symbols absent from the list); a repeat has no further effect.
- **Open World = true** — Mutates the Longbridge community sharelist service.

### update_watchlist_group
Implementation: `src/tools/quote.rs::update_watchlist_group` (`RequestUpdateWatchlistGroup`, modes add/remove/**replace**).

- **Read Only = false** — Updates a watchlist group by id (rename, or modify securities).
- **Destructive = true** — The `replace` mode overwrites the group's existing securities, and a rename overwrites the prior name — both can destroy/overwrite existing data, so the destructive hint is warranted.
- **Idempotent = true** — Re-applying the same update (same `id`, same `name`/`securities`/`mode`) yields the same final group state; no incremental effect from repetition.
- **Open World = true** — Mutates the Longbridge watchlist service.

### replace_order
Implementation: `src/tools/trade.rs::replace_order` (`ReplaceOrderOptions::new(order_id, quantity)`).

- **Read Only = false** — Modifies an existing open order's quantity / price / trigger / trailing parameters.
- **Destructive = true** — It overwrites the existing order's terms in place; the prior order parameters are replaced and not recoverable from this call.
- **Idempotent = true** — Re-applying the same replacement parameters to the same `order_id` produces the same resulting order state; repeating does not stack additional changes.
- **Open World = true** — Mutates state on the Longbridge trade gateway.

### dca_update
Implementation: `src/tools/dca.rs::dca_update` (`DcaUpdateParam`).

- **Read Only = false** — Updates an existing DCA plan by `plan_id` (amount/frequency/schedule).
- **Destructive = true** — It overwrites the plan's existing configuration; prior values are replaced, so it can overwrite existing data.
- **Idempotent = true** — Applying the same update parameters again to the same `plan_id` converges to the same plan configuration; no extra effect.
- **Open World = true** — Mutates the Longbridge DCA service.

### sharelist_sort
Implementation: `src/tools/sharelist.rs::sharelist_sort` (`SharelistItemsParam`).

- **Read Only = false** — Reorders securities within a community sharelist.
- **Destructive = true** — It overwrites the list's existing ordering with the supplied order — the previous arrangement is replaced.
- **Idempotent = true** — Applying the same target ordering again yields the same final order; a repeat is a no-op on state.
- **Open World = true** — Mutates the Longbridge community sharelist service.

---

## Section 3 — Non-destructive idempotent toggles (state switches)

Declared values: `read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true`.

### alert_enable
Implementation: `src/tools/alert.rs::alert_enable`.

- **Read Only = false** — Sets an alert's enabled flag to `true` on the server.
- **Destructive = false** — A reversible toggle; it changes a status flag and does not delete or overwrite alert data (it can be turned off again with `alert_disable`).
- **Idempotent = true** — Enabling an already-enabled alert leaves the same end state (`enabled: true`); repeating adds no effect.
- **Open World = true** — Mutates the Longbridge alert service.

### alert_disable
Implementation: `src/tools/alert.rs::alert_disable`.

- **Read Only = false** — Sets an alert's enabled flag to `false`.
- **Destructive = false** — Reversible toggle; the alert is preserved and can be re-enabled.
- **Idempotent = true** — Disabling an already-disabled alert leaves the same end state (`enabled: false`).
- **Open World = true** — Mutates the Longbridge alert service.

### dca_pause
Implementation: `src/tools/dca.rs::dca_pause`.

- **Read Only = false** — Suspends a DCA plan (stops execution until resumed).
- **Destructive = false** — Reversible: the description explicitly pairs it with `dca_resume`; the plan is preserved, only its run state is suspended. (Contrast with the irreversible `dca_stop`, which is destructive.)
- **Idempotent = true** — Pausing an already-paused plan leaves the same suspended state.
- **Open World = true** — Mutates the Longbridge DCA service.

### dca_resume
Implementation: `src/tools/dca.rs::dca_resume`.

- **Read Only = false** — Resumes a suspended DCA plan's scheduled execution.
- **Destructive = false** — Reversible toggle (can be paused again); preserves the plan.
- **Idempotent = true** — Resuming an already-active plan leaves the same active state.
- **Open World = true** — Mutates the Longbridge DCA service.

---

## Section 4 — Destructive non-idempotent tool (irreversible new side effect)

Declared values: `read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = true`.

### submit_order
Implementation: `src/tools/trade.rs::submit_order` (`SubmitOrderOptions::new(...)` then submit).

- **Read Only = false** — Submits a live buy/sell order to the brokerage.
- **Destructive = true** — Placing an order has a real, irreversible market side effect (it can execute and move cash/positions); it is not a reversible or purely-additive bookkeeping change, so the destructive hint is appropriate for an action this consequential.
- **Idempotent = false** — Each call creates a brand-new order (no client-side dedupe key). Calling twice with identical parameters submits two separate orders — an additional, potentially costly side effect — so it is correctly non-idempotent.
- **Open World = true** — Submits to the Longbridge trade gateway.

---

## Verification summary

All annotation values in this document were taken verbatim from
`src/tools/mod.rs` and matched the implementations in `quote.rs`, `trade.rs`,
`alert.rs`, `dca.rs`, `content.rs`, `sharelist.rs`, and `authenticate.rs`.
No discrepancies were found between the declared annotation values and the
prompt's stated values.
