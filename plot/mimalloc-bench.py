import plotly.graph_objects as go
import sys
from collections import defaultdict

benchmarks = set()
data = defaultdict(lambda: defaultdict(lambda: defaultdict(float)))

with open(sys.argv[1]) as file:
    for row in file.read().strip().splitlines():
        benchmark, allocator, time, rss, user, sys, faults, reclaims = row.split()
        benchmarks.add(benchmark)
        slot = data[benchmark][allocator]

        segments = time.split(":")
        if len(segments) > 1:
            slot["time"] = int(segments[0]) * 60 + float(segments[1])
        else:
            slot["time"] = float(time)

        slot["rss"] = float(rss)


benchmarks = list(sorted(benchmarks))
allocators = ["mi2", "cxlalloc", "cxl-shm", "r"]
bars = []
mins = [data[benchmark]["mi2"]["time"] for benchmark in benchmarks]

print(f"Benchmark", end="")
for allocator in allocators:
    print(f" & {allocator} (s)", end="")
print(" \\\\")

for i, benchmark in enumerate(benchmarks):
    print(f"{benchmark}", end="")

    for allocator in allocators:
        time = data[benchmark][allocator]["time"]
        if allocator == "mi2":
            print(f" & {time:.02f}", end="")
        elif time > 0.0001:
            print(f" & {time:.02f} ({time / mins[i]:.02f}x)", end="")
        else:
            print(" & ", end="")
    print(" \\\\")


# for allocator in allocators:
#     bars.append(
#         go.Bar(
#             name=allocator,
#             x=benchmarks,
#             y=[
#                 data[benchmark][allocator]["time"] / mins[i]
#                 for (i, benchmark) in enumerate(benchmarks)
#             ],
#             text=[data[benchmark][allocator]["time"] for benchmark in benchmarks],
#         )
#     )
#
# figure = go.Figure(bars)
# figure.update_layout(barmode="group")
# figure.update_yaxes(type="log")
# figure.update_layout(yaxis_range=[0, 2])
# figure.show()
