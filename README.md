# ltapiserv-rs

Server implementation of the LanguageTool API for **offline grammar and spell checking**, based on:

- https://github.com/bminixhofer/nlprule for grammar and style checking, using the [LanguageTool rules](https://github.com/languagetool-org/languagetool).
- https://github.com/reneklacan/symspell for spell-checking

This also contains a simple command-line client, displaying results graphically with [ariadne](https://docs.rs/ariadne/latest/ariadne/index.html).

See <https://c.pgdm.ch/eps-projects/ltapiserv-rs>

## Installation

The recommended method is to get a binary from the [releases page](https://github.com/cpg314/ltapiserv-rs/releases).

The `.deb` and Arch Linux packages will install a service definition in `/usr/lib/systemd/user/ltapiser-rs.service`, and it should suffice to enable it with

```console
$ systemctl --user enable --now ltapiserv-rs
```

A path to a custom dictionary can be passed to the server via the `--dictionary` option. The default `systemd` configuration places it in `~/dictionary.txt`.

### tar.gz archive

```console
$ sudo cp ltapiserv-rs /usr/local/bin
$ sudo chmod +x /usr/local/bin/ltapiserv-rs
$ ln -s $(pwd)/ltapiserv-rs.service ~/.config/systemd/user/ltapiserv-rs.service
$ systemctl --user daemon-reload && systemctl --user enable --now ltapiserv-rs
$ systemctl --user status ltapiserv-rs
```

### From source

Alternatively, binaries can be built from source as follows:

```console
$ # Create en_US.tar.gz data archive (will be embedded in the binary).
$ cargo make create-archive
$ cargo build --release
```

## Usage

The following clients have been tested. The server should be compatible with others, but there might be idiosyncrasies; don't hesitate to send a PR.

### Browser extension

Install the official LanguageTool browser extension (e.g. for [Chrome](https://languagetool.org/chrome) or [Firefox](https://languagetool.org/firefox)) and configure it to use your local server:

![Chrome extension settings](doc/chrome_ext.png)

### Command line client

A command line client, `ltapi-client`, is also included in this codebase.

```console
$ cat text.txt | ltapi-client --server http://localhost:8875
$ ltapi-client --server http://localhost:8875 test.txt
```

![Command line interface](doc/client.png)

The return code will be `1` if any error is detected.

The server address can be configured through the `LTAPI_SERVER` environment variable.

Formats like Markdown, HTML, LaTeX etc. can be processed through `pandoc`:

```console
$ pandoc README.md -t plain | ltapi-client
```

### flycheck-languagetool (emacs)

See <https://github.com/emacs-languagetool/flycheck-languagetool>

```emacs-lisp
(use-package flycheck-languagetool
  :ensure t
  :hook (text-mode . flycheck-languagetool-setup)
  :init
  (setq flycheck-languagetool-url "http://127.0.0.1:8875")
)
```

### ltex-ls (language server protocol for markup)

See <https://github.com/valentjn/ltex-ls>.

This currently requires [this patch](https://github.com/valentjn/ltex-ls/pull/276) to send the proper content type in the requests (this also could be done in `ltapiserv-rs` with an axum middleware to edit the content type).

Use the `ltex.languageToolHttpServerUri` variable to set the URL, e.g. with [lsp-ltex](https://github.com/emacs-languagetool/lsp-ltex) in emacs:

```emacs-lisp
(use-package lsp-ltex
  :ensure t
  :hook (text-mode . (lambda ()
                       (require 'lsp-ltex)
                       (lsp)))  ; or lsp-deferred
  :init
  (setq lsp-ltex-version "16.0.0"
        lsp-ltex-languagetool-http-server-uri "http://localhost:8875"
        )
)
```

## TODO

- [ ] Dynamic editing of the dictionary (`/words` endpoint).
- [ ] Tests
