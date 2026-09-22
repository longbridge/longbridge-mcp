# Error envelope

Failed calls return `isError: true` with a JSON body: `error_code`, `message`, `recoverable`, `hint`, `data`.
`recoverable`: `reauth` (re-authenticate, then retry), `backoff` (wait, then retry), `fix_params` (fix arguments, then retry), `none` (do not retry; tell the user).
Terminal no-access / no-data conditions may return `isError: false` with an explanatory envelope and empty placeholder fields; do not present those as real values.
Pipeline-specific codes: `invalid_execute`, `invalid_pipeline`, `unknown_tool` (with `data.suggestions`), `tool_unavailable_in_region`, `jq_filter_error`, `pipeline_timeout`.
