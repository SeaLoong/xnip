# xnip storage for OpenHands SDK

Install this local Python package together with `../native-python`, call
`register_xnip_file_editor()`, and select
`Tool(name="xnip_file_editor", params={"executable": "/absolute/path/to/xnip"})`
in an OpenHands SDK conversation. The resulting model-visible tool is the
official `file_editor`; xnip supplies guarded storage transactions.

Verified with OpenHands SDK/tools 1.60.1 and Python 3.12+. Configuration, restore,
host newline/encoding rules and actual SDK E2E are in the
[integration guide](../../docs/openhands-native.md).
