# ltapiserv-rs

Server implementation of the LanguageTool API for offline grammar and spell checking, based on:

- https://github.com/bminixhofer/nlprule for grammar and style checking, using the [LanguageTool rules](https://github.com/languagetool-org/languagetool).
- https://github.com/reneklacan/symspell for spell-checking

See <https://c.pgdm.ch/notes/ltapiserv-rs>

## Installation

Build with Cargo:

```console
$ # Create en_US.tar.gz data archive (will be embedded in the binary).
$ bash create_archive.sh
$ cargo build --release
```

### Systemd service

```console
$ sudo cp target/release/ltapiserv-rs /usr/local/bin
$ sudo chmod +x /usr/local/bin/ltapiserv-rs
$ ln -s $(pwd)/ltapiserv-rs.service ~/.config/systemd/user/ltapiserv-rs.service
$ systemctl --user daemon-reload && systemctl --user restart ltapiserv-rs
$ systemctl --user status ltapiserv-rs
```

## Usage

### Browser extension

Install the offical LanguageTool browser extension (e.g. for [Chrome](https://languagetool.org/chrome) or [Firefox](https://languagetool.org/firefox)) and configure it to use your local server:

![Chrome extension settings](chrome_ext.png)

### Flycheck (emacs)

```emacs-lisp
(use-package flycheck-languagetool
  :ensure t
  :hook (text-mode . flycheck-languagetool-setup)
  :init
  (setq flycheck-languagetool-url "http://127.0.0.1:8875")
)
```
