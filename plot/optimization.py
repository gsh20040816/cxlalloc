import sys
import common
import polars as pl


def main():
    pl.Config.set_tbl_rows(-1)

    opt = pl.scan_ndjson(sys.argv[1]).filter(
        pl.col("allocator").struct["name"] == "cxlalloc"
    )
    nobit = pl.scan_ndjson(sys.argv[2]).filter(
        pl.col("allocator").struct["name"] == "cxlalloc"
    )
    nomemcpy = pl.scan_ndjson(sys.argv[3]).filter(
        pl.col("allocator").struct["name"] == "cxlalloc"
    )

    opt = (
        common.collapse(opt)
        .drop_nulls()
        .select(
            common.PROCESS_COUNT,
            common.THREAD_COUNT,
            common.WORKLOAD,
            pl.col(common.THROUGHPUT).alias("tput_opt"),
        )
    )

    nobit = common.collapse(nobit).select(pl.col(common.THROUGHPUT).alias("tput_nobit"))

    nomemcpy = common.collapse(nomemcpy).select(
        pl.col(common.THROUGHPUT).alias("tput_nomemcpy")
    )

    df = (
        pl.concat(
            [opt, nobit, nomemcpy],
            how="horizontal",
        )
        .with_columns(
            opt_over_nobit=pl.col("tput_opt") / pl.col("tput_nobit"),
            opt_over_nomemcpy=pl.col("tput_opt") / pl.col("tput_nomemcpy"),
        )
        .collect()
    )

    print(df)


if __name__ == "__main__":
    main()
