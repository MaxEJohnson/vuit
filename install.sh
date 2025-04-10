#!/bin/bash

# Install the required packages

GREEN='\033[0;32m'
NC='\033[0m'
MAC_VERSION=0.6.4
LINUX_VERSION=0.6.4

if [[ $OSTYPE == darwin* ]]; then
    echo -e "${GREEN}Installing dependencies...${NC}"
    if [ $(which rg) ]; then
        echo -e "${GREEN}ripgrep already installed${NC}"
    else
        echo -e "${GREEN}Installing ripgrep...${NC}"
        brew install ripgrep 
    fi
fi

echo -e "${NC}Installing vuit${NC}"
if [[ $OSTYPE == linux-* ]]; then
    curl -L -O https://github.com/MaxEJohnson/vuit/releases/download/v${LINUX_VERSION}/vuit_${LINUX_VERSION}_amd64.deb
    sudo dpkg -i vuit_${LINUX_VERSION}_amd64.deb
else
    curl -L -O https://github.com/MaxEJohnson/vuit/releases/download/v${MAC_VERSION}/vuit-v${MAC_VERSION}-macos-arm64.tar.gz
    tar -xvf vuit-v${MAC_VERSION}-macos-arm64.tar.gz
    sudo mkdir -p /usr/local/bin
    sudo mv vuit /usr/local/bin/vuit
    sudo chmod +x /usr/local/bin/vuit
    sudo xattr -d com.apple.quarantine /usr/local/bin/vuit 2> /dev/null
    rm vuit-v${MAC_VERSION}-macos-arm64.tar.gz
fi

echo -e "${GREEN}vuit installed${NC}"
