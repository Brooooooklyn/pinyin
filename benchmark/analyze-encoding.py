"""Generate a compact CSV from persisted complete-call benchmark samples."""
import csv
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
folder = root / "benchmark/results/encoding"
data = json.loads((folder / "conversion.json").read_text())
rows = []
for row in data["rows"]:
    values = {v["implementation"]: v["medianNs"] / 1000 for v in row["result"]}
    before, after = values["before"], values["after"]
    rows.append({
        **{key: row[key] for key in ["fixture", "resolver", "style", "api", "inputType"]},
        "beforeUs": round(before, 3), "afterUs": round(after, 3),
        "lessTimePercent": round(100 * (1 - after / before), 2),
        "pinyinProUs": round(values["pinyin-pro"], 3) if "pinyin-pro" in values else "",
        "speedupVsPro": round(values["pinyin-pro"] / after, 3) if "pinyin-pro" in values else "",
        "matchesPro": row.get("matchesPro"),
    })
with (folder / "conversion.csv").open("w") as output:
    writer = csv.DictWriter(output, fieldnames=rows[0].keys())
    writer.writeheader()
    writer.writerows(rows)
for row in rows:
    if row["inputType"] == "string" and row["style"] == 1 and row["fixture"].endswith("100k"):
        print(row)
print(f"{len(rows)} cases; {sum(len(r['samples']) for row in data['rows'] for r in row['result'])} persisted samples")
