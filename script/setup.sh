#!/usr/bin/env bash

set -o errexit
set -o nounset
set -o pipefail
set -o xtrace

cd ~

if command -v nix &>/dev/null; then
    echo "Skipping nix installation"
else
    bash <(curl -L https://nixos.org/nix/install) --no-daemon
    source ~/.nix-profile/etc/profile.d/nix.sh
fi

if command -v direnv &>/dev/null; then
    echo "Skipping direnv installation"
else
    curl -sfL https://direnv.net/install.sh | sudo -E bin_path=/usr/bin bash
fi

grep -q direnv ~/.bashrc || echo 'eval "$(direnv hook bash)"' >> ~/.bashrc

[ -d "cxlalloc" ] || git clone '<REDACTED>'
cd cxlalloc
git submodule update --init --recursive

[ -f .envrc ] || echo "use flake" > .envrc
direnv allow .

cd twitter
[ -f 'memcached.tar.gz' ] || wget -O memcached.tar.gz 'https://www.dropbox.com/scl/fi/764zhtr3kvgf44fk6jihm/memcached.tar.gz?rlkey=6w9ehwzd68z52bhcb3mutadog&st=c861shpc&dl=1'
[ -f 'cluster37.000.parquet' ] || tar -xf memcached.tar.gz
cd ..

./script/normalize.sh
