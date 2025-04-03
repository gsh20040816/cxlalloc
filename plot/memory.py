import sys
import polars as pl
import polars.selectors as cs
import polars.datatypes as dt
import plotly.express as px

SCHEMA = {
    "ts": dt.Int64,
    "heap": dt.Categorical(),
    "name": dt.Categorical(),
    "thread": dt.UInt16(),
    "class": dt.UInt64(),
    "size": dt.Int64(),
}


def main():
    df = downsample(
        pl.scan_csv(
            sys.argv[1],
            has_header=False,
            schema=SCHEMA,
        ).sort(by=pl.col("ts"))
    )

    min_ts = df.select("ts").min().collect().item()

    df = df.select(
        pl.col("ts").sub(min_ts),
        ~cs.by_name("ts"),
    )

    integral = (
        df.group_by("name")
        .agg(pl.col("ts") / 1e6, pl.col("size").cum_sum())
        .explode("ts", "size")
        .sort("ts")
        .collect()
    )

    fig = px.line(integral, x="ts", y="size", color="name", facet_row="name")

    # fig = sp.make_subplots(rows=len(names), shared_xaxes=True)

    # for row, name in enumerate(names):
    #     integral = (
    #         df.filter(pl.col("name") == name)
    #         .select(pl.col("ts") / 1e6, pl.col("size").cum_sum())
    #         .collect()
    #     )
    #
    #     # Plot integral per owner
    #     fig.add_trace(
    #         go.Scatter(
    #             x=integral["ts"],
    #             y=integral["size"],
    #             name=name,
    #         ),
    #         col=1,
    #         row=row + 1,
    #     )
    #
    #     # Split by thread
    #     # for thread in data["thread"].unique():
    #     #     by_thread = data.filter(pl.col("thread") == thread)
    #     #     fig.add_trace(
    #     #         go.Scatter(
    #     #             x=by_thread["ts"] / 1e6,
    #     #             y=by_thread["size"].cum_sum(),
    #     #             name=name + "_" + str(thread),
    #     #         )
    #     #     )
    #     #
    #
    #     # Split by size class
    #     # data = df.filter(pl.col("heap") == "small") == name)
    #     # if data["class"].is_null().all():
    #     #     fig.add_trace(
    #     #         go.Scatter(x=data["ts"] / 1e6, y=data["size"].cum_sum(), name=name)
    #     #     )
    #     # else:
    #     #     for size in data["class"].unique():
    #     #         by_size = data.filter(pl.col("class") == size)
    #     #         if (by_size["size"] > 0).any():
    #     #             fig.add_trace(
    #     #                 go.Scatter(
    #     #                     x=by_size["ts"] / 1e6,
    #     #                     y=by_size["size"].cum_sum(),
    #     #                     name=name + "_" + str(size),
    #     #                 )
    #     #             )

    fig.show()
    fig.write_html("trace-integral.html", include_plotlyjs="cdn", include_mathjax=False)

    # fig = px.line(df.collect(), x="ts", y="size", color="name")
    # fig.show()
    # fig.write_html("trace-derivative.html", include_plotlyjs="cdn", include_mathjax=False)


def downsample(df, interval=100):
    return (
        df.group_by_dynamic(
            "ts", group_by=cs.exclude("ts", "size"), every=f"{interval}i"
        )
        .agg(pl.col("size").sum())
        .sort("ts")
    )


if __name__ == "__main__":
    main()
