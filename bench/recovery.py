import glob
import os
import subprocess as sp

# https://stackoverflow.com/questions/5137497/find-the-current-directory-and-files-directory
ROOT = os.path.dirname(os.path.realpath(__file__))
OBJECTS = 100000
ITERATIONS = 1


def main():
    compile("ralloc")

    for block in [True, False]:
        # Vary size at 1 crash
        for size in [30, 31, 32, 33, 34]:
            crash("ralloc", block, 1, size)

        # Vary crash count at 1GiB size
        for count in [1, 2, 4, 8, 16]:
            crash("ralloc", block, count, 30)

    compile("cxlalloc")

    # Vary size at 1 crash
    for size in [30, 31, 32, 33, 34]:
        crash("cxlalloc", False, 1, size)

    # Vary crash count at 1GiB size
    for count in [1, 2, 4, 8, 16]:
        crash("cxlalloc", False, count, 30)


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
                "/usr/bin/time",
                "-f",
                "%E %M %U %S %F %R",
                f"{ROOT}/../target/release/cxlalloc-recover",
                "--thread",
                "41",
                "--crash",
                ",".join(crashes),
                "--objects",
                OBJECTS,
                "--path",
                "/dev/shm/pool",
                "--threads",
                "40",
                "--size",
                str(2**size),
            ],
            capture_output=True,
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
        args.append("--feature")
        args.append("cxlalloc-recover/cxlalloc")

    sp.run(args)


if __name__ == "__main__":
    main()
