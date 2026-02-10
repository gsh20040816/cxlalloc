KDIR = /lib/modules/$(shell uname -r)/build
obj-m += mcas_target_buff.o
PWD := $(CURDIR)

all:
	make -C $(KDIR) M=$(PWD) modules

clean:
	make -C $(KDIR) M=$(PWD) clean
