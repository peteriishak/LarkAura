import json, sys
r = json.load(open(sys.argv[1]))
for k, v in r.items():
    if k != "state_at_error":
        print(f"{k}: {json.dumps(v)}")
s = r.get("state_at_error")
if s:
    print({k: s[k] for k in s if k != "windows"})
    for w in s["windows"]:
        print(w)
