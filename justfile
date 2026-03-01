demo:
    @fc-list | grep -qi "JetBrains Mono" || { echo "error: JetBrains Mono font not installed"; exit 1; }
    vhs demo/demo.tape
