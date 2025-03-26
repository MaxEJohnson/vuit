#!/bin/bash

# Install the required packages

GREEN='\033[0;32m'
NC='\033[0m'
VERSION=0.1.0

echo -e "${NC}Installing fd and fzf${NC}"
if [[ $OSTYPE == "*linux*" ]]; then
    sudo apt install fdfind
    sudo apt install fzf
else
    brew install fd
    brew install fzf
fi

echo -e "${GREEN}Dependencies installed${NC}"

echo -e "${NC}Installing vuit${NC}"
if [[ $OSTYPE == "*linux*" ]]; then
    curl -L -O https://github.com/MaxEJohnson/vuit/releases/download/v$VERSION/vuit_$VERSION_amd64.deb
    sudo dpkg -i vuit_$VERSION_amd64.deb
else
    curl -L -O https://github.com/MaxEJohnson/vuit/releases/download/v$VERSION/vuit-v$VERSION-macos-arm64.tar.gz
    tar -xvf vuit-v$VERSION-macos-arm64.tar.gz
    sudo mkdir -p /usr/local/bin
    sudo mv vuit /usr/local/bin/vuit
    sudo chmod +x /usr/local/bin/vuit
    sudo xattr -d com.apple.quarantine /usr/local/bin/vuit 2> /dev/null
    rm vuit-v$VERSION-macos-arm64.tar.gz
fi

echo -e "${GREEN}vuit installed${NC}"
