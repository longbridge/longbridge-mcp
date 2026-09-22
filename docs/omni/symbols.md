# Symbol format

`<CODE>.<MARKET>`: `700.HK`, `AAPL.US`, `600519.SH`, `000001.SZ`, `D05.SG`. Use the canonical code: `00700.HK` returns an empty record, not an error. Options use OCC-style symbols such as `AAPL230317P160000.US`; obtain them from `option_chain_expiry_date_list` then `option_chain_info_by_date`. Warrants and indices use the same `<CODE>.<MARKET>` form (e.g. `HSI.HK`).
