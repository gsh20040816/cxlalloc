import common
import plotly.graph_objects as go
import sys

def main():
    data = None
    name = sys.argv[2]
    with open(sys.argv[1]) as file:
        rows = [row.split(",") for row in file.read().strip().splitlines()]
        data = [(int(thread), name, int(count)) for thread, name, count in rows]

    for index, prefix in enumerate(["ALLOCATE", "FREE", "BUMP", "GLOBAL"]):
        labels, sources, targets, values = parse([row for row in data if row[1].startswith(prefix)])
        figure = go.Figure(data=[go.Sankey(
            node=dict(
                label=labels,
                pad=10,
            ),
            link=dict(
                arrowlen=20,
                source=sources,
                target=targets,
                value=values,
            )
        )])

        figure.update_layout(title=f"{name}-{prefix.lower()}")
        figure.write_html(f"{name}-{prefix.lower()}.html")
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
            labels.append(f"{name}<br>{common.display_count(count)}")
            continue
        else:
            parent = data[prefix]
            labels.append(f"{name}<br>{common.display_count(count)} ({count / parent * 100:.02f}%)")

        sources.append(indexes[prefix])
        targets.append(indexes[name])
        values.append(count)

    return labels, sources, targets, values


if __name__ == "__main__":
    main()
