import math
import plotly.graph_objects as go
import sys

def main():
    data = None
    with open(sys.argv[1]) as file:
        rows = [row.split(",") for row in file.read().splitlines()]
        data = [(int(row[0]), row[1], int(row[2])) for row in rows]

    labels, sources, targets, values = parse(data)

    figure = go.Figure(data=[go.Sankey(
        node=dict(
            label=labels,
        ),
        link=dict(
            source=sources,
            target=targets,
            value=values,
        )
    )])

    figure.show()


def parse(data: list[tuple[int, str, int]]):
    # Remove events with zero count
    data = [row for row in data if row[2] > 0]

    # Aggregate across all threads
    names = sorted({ row[1] for row in data })
    data = { name: sum([ row[2] for row in data if row[1] == name ]) for name in names }

    # Nodes
    labels = []
    indexes = {}

    # Edges
    sources = []
    targets = []
    values = []

    for name, count in data.items():
        prefix = name.rsplit("_", 1)[0]
        suffix = name.rsplit("_", 1)[-1]

        # Lookup from name to label index
        indexes[name] = len(labels)

        # Root node
        if prefix == suffix:
            labels.append(f"{name}<br>{display(count)}")
            continue
        else:
            parent = data[prefix]
            labels.append(f"{name}<br>{display(count)} ({count / parent * 100:.02f}%)")

        sources.append(indexes[prefix])
        targets.append(indexes[name])
        values.append(count)

    return labels, sources, targets, values


def display(value: int) -> str:
    suffixes = ["", "K", "M", "B", "T"]
    if value == 0:
        return "0"

    index = int(math.log10(value) / 3)
    if index == 0:
        return f"{value}"
    else:
        return f"{value / (10**(3 * index)):.02f}{suffixes[index]}"


if __name__ == "__main__":
    main()
