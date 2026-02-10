sudo rmmod mcas_target_buff
dmesg -C
dmesg -D
make -j8
dmesg -E
sudo insmod mcas_target_buff.ko $*
dmesg -D
#make clean
#dmesg -E
#rmmod ioat_map
