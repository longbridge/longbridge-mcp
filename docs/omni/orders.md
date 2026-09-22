# Orders are two-step

`submit_order`, `submit_multileg_order`, `replace_order`, `cancel_order` and all `grid_*` writes are DRY RUN when called without `execute`: they validate, echo the order, and return a one-time `confirmation_code`. Show that preview to the user. Only after the user explicitly confirms, call the same tool again with `execute` set to the code. The code is derived from the order itself, so changing symbol, side, quantity or price invalidates it. Never retry a write automatically after an error; verify its outcome first (`today_orders`, `order_detail`).
