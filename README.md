### 1. install gnostr

```
cargo install gnostr

```

### 2. run

```
WEEBLE=$(gnostr --weeble); \
BLOCKHEIGHT=$(gnostr --blockheight); \
WOBBLE=$(gnostr --wobble); \
cargo -q run > $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
git add . && \
gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow beef

```
