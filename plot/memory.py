import sys
import polars as pl
import polars.selectors as cs
import polars.datatypes as dt
import plotly.graph_objects as go

SCHEMA = {
    "ts": dt.Int64,
    "heap": dt.Categorical(),
    "name": dt.Categorical(),
    "thread": dt.UInt16(),
    "class": dt.UInt64(),
    "size": dt.Int64(),
}

original = pl.read_csv(
    sys.argv[1],
    has_header=False,
    schema=SCHEMA,
).sort(by=pl.col("ts"))

min_ts = original.select("ts").min()

df = original.select(
    pl.col("ts").sub(min_ts),
    ~cs.by_name("ts"),
)

fig = go.Figure()

for name in df["name"].unique():
    # if name in ["data", "slab_local", "slab_remote"]:
    #     continue

    data = df.filter((pl.col("heap") == "small") & (pl.col("name") == name))

    # print(data)

    # if name in ["detached", "global_unsized"]:
    #     fig.add_trace(
    #         go.Scatter(x=data["ts"] / 1e6, y=data["size"].cum_sum(), name=name)
    #     )
    #     continue
    #
    # for thread in data["thread"].unique():
    #     by_thread = data.filter(pl.col("thread") == thread)
    #     fig.add_trace(
    #         go.Scatter(
    #             x=by_thread["ts"] / 1e6,
    #             y=by_thread["size"].cum_sum(),
    #             name=name + "_" + str(thread),
    #         )
    #     )
    #
    if data["class"].is_null().all():
        fig.add_trace(
            go.Scatter(x=data["ts"] / 1e6, y=data["size"].cum_sum(), name=name)
        )
    else:
        for size in data["class"].unique():
            by_size = data.filter(pl.col("class") == size)
            if (by_size["size"] > 0).any():
                fig.add_trace(
                    go.Scatter(
                        x=by_size["ts"] / 1e6,
                        y=by_size["size"].cum_sum(),
                        name=name + "_" + str(size),
                    )
                )

fig.show()

print(
    original.group_by("name")
    .agg(pl.col("ts"), pl.col("size").cum_sum())
    .explode(["ts", "size"])
    .sort("ts")
    .group_by_dynamic("ts", every="1us")
    .agg(
        a=pl.col("size").filter(pl.col("name") == "application").max(),
        d=pl.col("size").filter(pl.col("name") == "data").max(),
    )
)
