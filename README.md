# vaiber

> p2p identity. No blockchains, no fees. Just cryptographic secure identity and search.

Imagine a world where you can find data and people even when thei contact data chnages. Connect with your friends once and for all. Forever be connected, rotate and recover your identity, and never lose contact with your friends.

- Create a Verifiable Long-lived Address (Vlad) and add details to your Plog... like a decentralized link-in-bio.
- Connect to anyone else'sVerifiable Long-Lived Address (VLAD), which stays the same even if they rotate their keys 
- [TODO] Search for stuff without even needing to download all their data... just the index!
- [TODO] Social recovery if you lose your main key

### Serving The App

First, launch the Tailwindcss build process to compile the styles:

```bash
just css
```

On the web, use [just command](https://just.systems/man/en/) to launch the web [recipe](./justfile):

```bash
just serve-desktop
```


To launch a second node on the same machine, use [just command](https://just.systems) to launch [recipe](./justfile):

```bash
just serve-second-desktop
```
```
