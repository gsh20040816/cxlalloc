#!/bin/bash
set -x
PCIE_BAR="43:00.1"

sudo setpci -s $PCIE_BAR COMMAND=0x02

cd ../kmods
PATH_LIST="kmod_mCAS_target_buff"
for path in $PATH_LIST; do
	cd $path
	sudo bash compile_install.sh
	cd ..
done
