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

### or

```
WEEBLE=$(gnostr --weeble); \
BLOCKHEIGHT=$(gnostr --blockheight); \
WOBBLE=$(gnostr --wobble); \
cargo -q run > $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
cargo -q run --bin utc_consensus >> $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
git add . && gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow beef

```

### or

```
WEEBLE=$(gnostr --weeble); \
BLOCKHEIGHT=$(gnostr --blockheight); \
WOBBLE=$(gnostr --wobble); \
cargo -q run > $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
cargo -q run --bin utc_consensus >> $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
git add . && \
gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow $(gnostr --weeble)
```

### or

```
WEEBLE=$(gnostr --weeble); \
BLOCKHEIGHT=$(gnostr --blockheight); \
WOBBLE=$(gnostr --wobble); cargo -q run > $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
cargo -q run --bin utc_consensus >> $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
git add . && \
gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow $(gnostr --wobble)
```

### or 

```
WEEBLE=$(gnostr --weeble); \
BLOCKHEIGHT=$(gnostr --blockheight); \
WOBBLE=$(gnostr --wobble); cargo -q run > $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
cargo -q run --bin utc_consensus >> $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
cargo -q run --bin byz_time >> $WEEBLE-$BLOCKHEIGHT-$WOBBLE.txt && \
git add . && \
gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow $WOBBLE && \
gnostr legit -m "$WEEBLE/$BLOCKHEIGHT/$WOBBLE" --pow $WEEBLE
```
