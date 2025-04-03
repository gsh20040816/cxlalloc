import altair as alt
import common
import polars as pl
import polars.selectors as cs
import sys

ALLOCATOR = "Allocator"
CRASH_COUNT = "Crash Count"
PHASE = "Phase"
PHASE_GC = "Garbage Collection"
PHASE_EXEC = "Execution"
WORKLOAD = "Workload"
LEAK_SIZE = "Leak Size"
TIME = "Execution Time (s)"
TIME_TOTAL = "Total Time"

ORDER_ALLOCATOR = ["cxlalloc", "ralloc-leak", "ralloc-block"]
ORDER_PHASE = [PHASE_GC, PHASE_EXEC]


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])

    df = (
        common.unnest_all(df)
        .select(
            pl.when(pl.col("config/allocator") == "cxlalloc")
            .then(pl.lit("cxlalloc"))
            .when((pl.col("config/allocator") == "ralloc") & pl.col("config/block"))
            .then(pl.lit("ralloc-gc"))
            .otherwise(pl.lit("ralloc-leak"))
            .alias(ALLOCATOR),
            pl.col("config/workload").alias(WORKLOAD),
            pl.col("config/crash_count").alias(CRASH_COUNT),
            pl.col("output/time").alias(TIME_TOTAL),
            pl.col("output/gc_time").alias(PHASE_GC),
            (pl.col("output/time") - pl.col("output/gc_time")).alias(PHASE_EXEC),
            pl.col("output/cache_size").alias(LEAK_SIZE),
        )
        .filter(pl.col(CRASH_COUNT).is_in([0, 1, 2]))
        .unpivot(
            index=cs.exclude(PHASE_GC, PHASE_EXEC),
            variable_name=PHASE,
            value_name=TIME,
        )
        .with_columns(
            pl.col(LEAK_SIZE) / 2**10,
            pl.col(TIME_TOTAL) / 1e6,
            pl.col(TIME) / 1e6,
        )
    )

    charts = []

    workloads = (
        df.select(WORKLOAD).unique(maintain_order=True).collect().get_column(WORKLOAD)
    )

    for col, workload in enumerate(workloads):
        data = df.filter(pl.col(WORKLOAD) == workload).collect()

        base = alt.Chart(data).encode(
            x=alt.X(f"{CRASH_COUNT}:O", title=None),
            y=alt.Y(f"{TIME}:Q", title=TIME if col == 0 else None),
            xOffset=alt.XOffset(f"{ALLOCATOR}:N").sort(ORDER_ALLOCATOR),
        )

        bar = base.mark_bar().encode(
            color=alt.Color(f"{ALLOCATOR}:N").sort(ORDER_ALLOCATOR),
            fillOpacity=alt.FillOpacity(PHASE).sort(ORDER_PHASE),
        )

        leak = (
            base.transform_filter(
                (alt.datum[ALLOCATOR] == "ralloc-leak")
                & (alt.datum[PHASE] == PHASE_EXEC)
            )
            .transform_calculate(
                text=alt.expr.if_(
                    alt.datum[LEAK_SIZE] > 0,
                    alt.expr.format(alt.datum[LEAK_SIZE], ".3s") + " KiB Leaked",
                    "",
                )
            )
            .mark_text(align="left", angle=270, dx=5)
            .encode(text="text:N")
        )

        gc = (
            base.transform_filter(
                (alt.datum[ALLOCATOR] == "ralloc-gc") & (alt.datum[PHASE] == PHASE_GC)
            )
            .transform_calculate(
                text=alt.expr.if_(
                    alt.datum[TIME] > 0,
                    alt.expr.format(alt.datum[TIME] / alt.datum[TIME_TOTAL], ".2p")
                    + " in GC",
                    "",
                )
            )
            .mark_text(align="left", angle=270, dx=5)
            .encode(y=alt.value(200), text="text:N")
        )

        inner = alt.layer(bar + leak + gc, title=alt.Title(workload)).properties(
            height=200, width=200
        )
        charts.append(inner)

    bar = alt.vconcat(
        alt.hconcat(
            *charts,
            title=alt.Title("Memento Workloads", align="center", anchor="middle"),
        ),
        alt.hconcat(
            title=alt.Title("Thread Crash Count", align="center", anchor="middle")
        ),
        center=True,
    ).configure_legend(orient="bottom")
    bar.show()


if __name__ == "__main__":
    main()
