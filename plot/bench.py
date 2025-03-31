import sys
import polars as pl
import altair as alt


def main():
    alt.renderers.enable("browser")

    df = pl.scan_ndjson(sys.argv[1])
    df = unnest_all(df, "/").sort(
        ["allocator", "config_benchmark/trace", "config_global/thread_count"]
    )

    baseline = (
        df.filter(pl.col("allocator") == "cxlalloc")
        .select("output/throughput")
        .collect()
        .to_series(0)
    )

    df = (
        df.group_by("allocator")
        .agg(
            trace=pl.col("config_benchmark/trace"),
            thread_count=pl.col("config_global/thread_count"),
            throughput=pl.col("output/throughput"),
            throughput_relative=pl.col("output/throughput") / baseline,
        )
        .explode(["trace", "thread_count", "throughput", "throughput_relative"])
        .filter(
            (
                (pl.col("allocator") == "cxl_shm")
                & pl.col("trace").is_in(
                    [
                        "./twitter/cluster12.000.parquet",
                        "./twitter/cluster37.000.parquet",
                    ]
                )
            ).not_()
        )
        .collect()
    )

    chart = df.plot.bar(
        x="thread_count:N",
        y="throughput_relative",
        color="allocator",
        xOffset="allocator",
    ).facet(column="trace")
    chart.show()


# https://github.com/pola-rs/polars/issues/12353
def unnest_all(df, separator="."):
    def _unnest_all(schema, separator):
        def _unnest(schema, path=[]):
            for name, dtype in schema.items():
                base_type = dtype.base_type()

                if base_type == pl.Struct:
                    yield from _unnest(dtype.to_schema(), path + [name])
                else:
                    yield path + [name], dtype

        for (col, *fields), dtype in _unnest(schema):
            expr = pl.col(col)

            for field in fields:
                expr = expr.struct[field]

            if col == "":
                name = separator.join(fields)
            else:
                name = separator.join([col] + fields)

            yield expr.alias(name)

    return df.select(_unnest_all(df.schema, separator))


if __name__ == "__main__":
    main()
