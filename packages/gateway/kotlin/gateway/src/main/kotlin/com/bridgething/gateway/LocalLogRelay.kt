package com.bridgething.gateway

public object LocalLogRelay {
    @Volatile private var sink: ((String, String, String) -> Unit)? = null

    public fun setSink(sink: ((String, String, String) -> Unit)?) {
        this.sink = sink
    }

    /**
     * Whether anything is listening. Producers check this before doing work to
     * build a line: with no stream open nothing may cross the native boundary,
     * and parsing every logcat line for a sink that will drop it is pure drain.
     */
    public fun hasSink(): Boolean = sink != null

    public fun push(level: String, target: String, message: String) {
        sink?.invoke(level, target, message)
    }
}
