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

[ -d "cxlalloc" ] || git clone https://github.com/nwtnni/cxlalloc.git
cd cxlalloc
git submodule update --init --recursive

[ -f .envrc ] || echo "use flake" > .envrc
direnv allow .

./script/normalize.sh
