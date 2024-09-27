import common
import sys
from collections import defaultdict

log = None
with open(sys.argv[1]) as file:
    log = file.read().splitlines()

data = defaultdict(lambda: defaultdict(int))
current = None

for line in log:
    if line[0].isalpha():
        current = line
        continue

    thread, *row = line.split(",")

    for size, count in [map(int, col.split(":")) for col in row]:
        data[current][size] += count

sizes = set()
for row in data.values():
    sizes = sizes.union(row.keys())

sizes = list(sorted(sizes))
times = None
with open(sys.argv[2]) as file:
    times = { row["benchmark"]: row["time"] for row in common.parse_mimalloc_bench(file.read())}

print("benchmark,total,time,throughput,", end="")
print(",".join(map(common.display_size, sizes)))

for name in sorted(data.keys()):
    total = sum(data[name].values())
    row = data[name]

    print(f"{name},\
{common.display_count(total)},\
{times[name]:.03f},\
{common.display_count(int(total / times[name]))},", end="")
    print(",".join([
        f"{row[size] / total * 100:.01f}% ({common.display_count(row[size])})"
        if row[size] > 0 else ""
        for size in sizes
    ]))
