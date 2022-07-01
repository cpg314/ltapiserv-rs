#!/usr/bin/env bash

set -exuo pipefail

mkdir en_US
wget -O- https://github.com/bminixhofer/nlprule/releases/download/0.6.4/en_tokenizer.bin.gz | gunzip -c > en_US/tokenizer.bin
wget -O- https://github.com/bminixhofer/nlprule/releases/download/0.6.4/en_rules.bin.gz | gunzip -c > en_US/rules.bin
wget https://github.com/reneklacan/symspell/raw/master/data/frequency_dictionary_en_82_765.txt -O en_US/frequency_dict.txt
tar czf en_US.tar.gz en_US/*
tar tvf en_US.tar.gz
