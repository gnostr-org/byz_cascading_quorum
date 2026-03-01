## [byz\_cascading\_quorum.rs](https://gitworkshop.dev/npub10xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqpkge6d/relay.damus.io/byz-cascading-quorum)

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
