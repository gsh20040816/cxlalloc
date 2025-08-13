import common
import sys
import polars as pl


def main():
    df = pl.scan_ndjson(sys.argv[1], infer_schema_length=None)

    df = common.collapse(
        df,
        workloads=common.MICRO_WORKLOADS,
    ).collect()

    fig = common.make_subplots(common.MICRO_WORKLOADS)
    common.update_layout(fig, full=False, numa=False)
    fig.show()


if __name__ == "__main__":
    main()
