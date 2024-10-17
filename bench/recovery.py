import glob
import os
import subprocess as sp

# https://stackoverflow.com/questions/5137497/find-the-current-directory-and-files-directory
ROOT = os.path.dirname(os.path.realpath(__file__))
OBJECTS = 1000000
ITERATIONS = 1


def main():
    compile("ralloc")

    for block in [True, False]:
        # Vary crash count at 16GiB size
        # Note: sometimes ralloc runs out of memory
        # for 4 crashes, 16GiB heap, non-blocking
        for count in [0, 1, 2, 4, 8]:
            crash("ralloc", block, count, 34)

    compile("cxlalloc")

    # Vary crash count at 16GiB size
    for count in [0, 1, 2, 4, 8]:
        crash("cxlalloc", False, count, 34)


def crash(allocator: str, block: bool, count: int, size: int):
    for i in range(ITERATIONS):
        for path in glob.glob("/dev/shm/pool*"):
            os.remove(path)

        print(
            f"Running {allocator}, block={block}, count={count}, size={size} ({i + 1}/{ITERATIONS})"
        )
        interval = OBJECTS // (count + 1)
        crashes = [interval * i for i in range(1, count + 1)]
        output = sp.run(
            [
                "env",
                "CXL_NUMA_NODE=2",
                "numactl",
                "--cpunodebind=1",
                "--membind=1",
                "/usr/bin/time",
                "-f",
                "%E %M %U %S %F %R",
                f"{ROOT}/../target/release/cxlalloc-recover",
                "--thread",
                "40",
                *(["--crash", ",".join(map(str, crashes))] if len(crashes) > 0 else []),
                "--objects",
                str(OBJECTS),
                "--path",
                "/dev/shm/pool",
                "--threads",
                "40",
                *(["--block"] if block else []),
                "--size",
                str(2**size),
            ],
            stdout=sp.PIPE,
            stderr=sp.STDOUT,
            text=True,
        )

        with open(
            f"{ROOT}/{allocator}-{'block' if block else 'leak'}-c{count}-s{size}-{i}.log",
            "w",
        ) as file:
            file.write(output.stdout)


def compile(allocator: str):
    args = [
        "cargo",
        "build",
        "--release",
        "--package",
        "cxlalloc-recover",
    ]

    if allocator == "cxlalloc":
        args.append("--features")
        args.append("cxlalloc-recover/cxlalloc")

    sp.run(args)


if __name__ == "__main__":
    main()
