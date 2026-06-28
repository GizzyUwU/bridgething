package com.bridgething.gateway

public object LocalLogRelay {
    @Volatile private var sink: ((String, String, String) -> Unit)? = null

    public fun setSink(sink: ((String, String, String) -> Unit)?) {
        this.sink = sink
    }

    public fun push(level: String, target: String, message: String) {
        sink?.invoke(level, target, message)
    }
}
