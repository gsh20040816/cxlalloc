#!/bin/bash
set -x

i=2
while [[ $i -le 32 ]]; do
	echo $i
	./mcas -t $i -i 100000
	i=$((i * 2))
done
