import math

def display_count(value: int) -> str:
    suffixes = ["", "K", "M", "B", "T"]
    if value == 0:
        return "0"

    index = int(math.log10(value) / 3)
    if index == 0:
        return f"{value}"
    else:
        return f"{value / (10**(3 * index)):.01f}{suffixes[index]}"


def display_size(value: int) -> str:
    suffixes = ["B", "KiB", "MiB", "GiB"]
    if value == 0:
        return ""

    index = int(math.log2(value) / 10)
    if index == 0:
        return f"{value}"
    else:
        return f"{value / (2**(10 * index)):.01f}{suffixes[index]}"


def parse_mimalloc_bench(data: str):
    rows = []

    for line in data.splitlines():
        benchmark, allocator, time, rss, user, sys, faults, reclaims = line.split()
        rows.append(dict(
            benchmark=benchmark,
            allocator=allocator,
            time=float(time),
            rss=int(rss),
            user=float(user),
            sys=float(sys),
            faults=int(faults),
            reclaims=int(reclaims)
        ))

    return rows

