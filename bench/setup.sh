#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

cd ~

bash <(curl -L https://nixos.org/nix/install) --no-daemon

curl -sfL https://direnv.net/install.sh | sudo -E bin_path=/usr/bin bash

echo 'eval "$(direnv hook bash)"' >> ~/.bashrc

git clone git@github.com:nwtnni/cxlalloc.git
cd cxlalloc
git submodule update --init --recursive

echo "use flake" > .envrc
direnv allow .

./bench/normalize.sh
