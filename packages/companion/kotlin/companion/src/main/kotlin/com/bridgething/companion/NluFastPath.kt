package com.bridgething.companion

import com.bridgething.schema.NluAmount
import com.bridgething.schema.NluDirection
import com.bridgething.schema.NluPlaybackSpeed
import com.bridgething.schema.NluRepeatMode
import com.bridgething.schema.NluView
import java.util.concurrent.ConcurrentHashMap
import java.util.regex.Pattern

object NluFastPath {
    data class Hit(val intent: String, val slots: NluMutableSlots)

    private val fillers = setOf(
        "uh", "uhh", "uhhh", "uhhhh", "um", "umm", "hmm", "hmmm",
        "er", "eh", "ah", "oh", "well", "so", "like", "yeah", "yep",
        "yes", "ok", "okay", "hey", "please", "thanks", "thank",
        "mean", "wait",
    )

    private val wordToNumber = mapOf(
        "zero" to 0, "one" to 1, "two" to 2, "three" to 3, "four" to 4, "five" to 5,
        "six" to 6, "seven" to 7, "eight" to 8, "nine" to 9, "ten" to 10,
        "eleven" to 11, "twelve" to 12, "thirteen" to 13, "fourteen" to 14, "fifteen" to 15,
        "sixteen" to 16, "seventeen" to 17, "eighteen" to 18, "nineteen" to 19,
        "twenty" to 20, "thirty" to 30, "forty" to 40, "fifty" to 50, "sixty" to 60,
        "seventy" to 70, "eighty" to 80, "ninety" to 90, "hundred" to 100,
    )

    private val generic = setOf(
        "the", "this", "that", "these", "those", "a", "an", "my", "our", "its", "it",
        "current", "currently", "some", "song", "songs", "track", "tracks", "music",
        "playback", "tune", "tunes", "playlist", "playlists", "can", "could", "would",
        "will", "shall", "you", "your", "i", "we", "us", "let", "lets", "want", "wanna",
        "need", "gotta", "must", "should", "may", "might", "now", "immediately", "for",
        "me", "of", "already", "right",
    )

    fun match(transcript: String): Hit? {
        val (raw, tokens) = normalize(transcript)
        if (tokens.isEmpty()) return null
        val core = tokens.filter { it !in generic }.joinToString(" ")
        for (rule in rules) {
            rule(tokens, raw, core)?.let { return it }
        }
        return null
    }

    private val punctuation = Pattern.compile("[^\\p{L}\\p{N}_\\s']")

    private fun normalize(transcript: String): Pair<String, List<String>> {
        val lowered = punctuation.matcher(transcript.lowercase()).replaceAll(" ")
        val tokens = lowered.split(" ").filter { it.isNotEmpty() && it !in fillers }
        return Pair(tokens.joinToString(" "), tokens)
    }

    private fun coreIsOnly(core: String, target: Set<String>, leads: Set<String> = emptySet()): Boolean {
        val tokens = core.split(" ").filter { it.isNotEmpty() }
        if (tokens.none { it in target }) return false
        return tokens.all { it in target || it in leads }
    }

    private fun parseInt(s: String): Int? {
        val cleaned = s.replace("-", " ").replace("percent", "").trim()
        if (cleaned.isEmpty()) return null
        val direct = cleaned.toIntOrNull()
        if (direct != null && direct in 0..100) return direct
        var total = 0
        for (p in cleaned.split(" ").filter { it.isNotEmpty() }) {
            val v = wordToNumber[p] ?: return null
            if (v == 100) total = maxOf(total, 1) * 100 else total += v
        }
        return if (total in 0..100) total else null
    }

    private val compiled = ConcurrentHashMap<String, Pattern>()

    private fun regex(pattern: String): Pattern =
        compiled.getOrPut(pattern) { Pattern.compile(pattern) }

    private fun contains(text: String, pattern: String): Boolean = regex(pattern).matcher(text).find()

    private fun captureGroup(text: String, pattern: String): String? {
        val m = regex(pattern).matcher(text)
        if (!m.find() || m.groupCount() < 1) return null
        return m.group(1)
    }

    private const val PRESET_RE = "preset\\s+(\\w+)"

    private fun rulePlayPreset(tokens: List<String>, raw: String, core: String): Hit? {
        if ("preset" !in tokens) return null
        val leadOk = tokens.firstOrNull() in setOf("play", "load", "switch", "go", "select") ||
            tokens.take(2) == listOf("go", "to")
        if (!leadOk) return null
        if ("save" in tokens || "store" in tokens) return null
        val n = captureGroup(raw, PRESET_RE)?.let { parseInt(it) } ?: return null
        if (n !in 1..4) return null
        val at = raw.indexOf("preset")
        if (at >= 0) {
            val trailing = raw.substring(at + "preset".length).trim().split(" ").filter { it.isNotEmpty() }
            if (trailing.size > 2) return null
        }
        return Hit("PRESET_PLAY", NluMutableSlots(preset = n.toString()))
    }

    private fun ruleSaveToPreset(tokens: List<String>, raw: String, core: String): Hit? {
        if ("preset" !in tokens) return null
        if ("save" !in tokens && "store" !in tokens) return null
        val n = captureGroup(raw, PRESET_RE)?.let { parseInt(it) } ?: return null
        if (n !in 1..4) return null
        return Hit("PRESET_SAVE", NluMutableSlots(preset = n.toString()))
    }

    private fun ruleVolumeAbsolute(tokens: List<String>, raw: String, core: String): Hit? {
        if ("volume" !in tokens && "level" !in tokens) return null
        val patterns = listOf(
            "(?:set|put)\\s+(?:the\\s+)?volume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s+percent|\\s*$|\\s+please\\b)",
            "\\bvolume\\s+([\\w\\s-]+?)\\s+percent\\b",
            "\\bvolume\\s+(?:to|at)\\s+([\\w\\s-]+?)(?:\\s*$|\\s+percent)",
            "volume\\s+(?:to|at)?\\s*(\\d+|[a-z]+)\\s*(?:percent)?\\s*$",
        )
        for (pat in patterns) {
            val n = captureGroup(raw, pat)?.let { parseInt(it) } ?: continue
            if (n in 1..100) return Hit("SET_VOLUME", NluMutableSlots(level = n.toUInt()))
        }
        return null
    }

    private val speedRules: List<Pair<NluPlaybackSpeed, List<String>>> = listOf(
        NluPlaybackSpeed.OnePointTwo to listOf(
            "\\bone\\s+point\\s+two(?:\\s+(?:speed|x|times))?\\b",
            "\\b1\\.2\\s*x?\\b",
            "\\b(?:play\\s+it\\s+|speed\\s+)faster\\b",
            "\\bspeed\\s+(?:it\\s+)?up\\b",
            "\\ba\\s+little\\s+faster\\b",
            "\\bfaster\\s+a\\s+little\\b",
        ),
        NluPlaybackSpeed.OnePointFive to listOf(
            "\\bone\\s+(?:and\\s+a\\s+)?half(?:\\s+speed)?\\b",
            "\\b1\\.5\\s*x?\\b",
            "\\bone\\s+point\\s+five\\b",
        ),
        NluPlaybackSpeed.One to listOf(
            "\\bnormal\\s+speed\\b",
            "\\b(?:back\\s+to\\s+|reset\\s+to\\s+)?(?:1\\s*x|one\\s+x|original\\s+speed)\\b",
            "\\b(?:play\\s+(?:it\\s+)?(?:at\\s+)?|at\\s+)normal(?:\\s+speed)?\\b",
        ),
        NluPlaybackSpeed.Two to listOf(
            "\\bdouble\\s+speed\\b",
            "\\b2\\s*x\\b",
            "\\btwo\\s+x\\b",
            "\\btwo\\s+times(?:\\s+speed)?\\b",
        ),
    )

    private fun rulePlaybackSpeed(tokens: List<String>, raw: String, core: String): Hit? {
        val anchors = listOf(
            "speed", "faster", "slower", "normal", "double", "2x", "1.5", "1.2",
            "two x", "two times", "one point", "half",
        )
        if (anchors.none { raw.contains(it) }) return null
        for ((speed, patterns) in speedRules) {
            for (p in patterns) {
                if (contains(raw, p)) return Hit("SET_PLAYBACK_SPEED", NluMutableSlots(speed = speed))
            }
        }
        return null
    }

    private fun ruleSeek(tokens: List<String>, raw: String, core: String): Hit? {
        val has15 = contains(raw, "\\b(?:15|fifteen)\\b")
        if (has15 && contains(raw, "\\b(?:rewind|go\\s+back|back|skip\\s+back)\\b")) {
            return Hit("SEEK_RELATIVE", NluMutableSlots(seconds = -15))
        }
        if (contains(raw, "^back\\s+fifteen\\s*$")) {
            return Hit("SEEK_RELATIVE", NluMutableSlots(seconds = -15))
        }
        if (has15 && contains(raw, "\\b(?:fast\\s+forward|forward|skip\\s+(?:ahead|forward))\\b")) {
            return Hit("SEEK_RELATIVE", NluMutableSlots(seconds = 15))
        }
        if (contains(raw, "\\bjump\\s+ahead\\b") &&
            contains(raw, "\\bjump\\s+ahead(?:\\s+(?:fifteen|15))?\\s*(?:seconds?)?\\s*$")
        ) {
            return Hit("SEEK_RELATIVE", NluMutableSlots(seconds = 15))
        }
        if (contains(raw, "^forward\\s+fifteen\\s*$")) {
            return Hit("SEEK_RELATIVE", NluMutableSlots(seconds = 15))
        }
        return null
    }

    private const val COLLECTION_RE = "\\b(?:playlist|album|queue|everything|all)\\b"

    private const val NAMED_COLLECTION_RE = "\\b(?:playlists?|albums?|stations?|podcasts?|artists?)\\b"

    private fun ruleRepeat(tokens: List<String>, raw: String, core: String): Hit? {
        if ("repeat" !in tokens && "loop" !in tokens && "looped" !in tokens &&
            !contains(raw, "\\bover\\s+and\\s+over\\b")
        ) {
            return null
        }
        if (raw.contains("shuffl")) return null

        if (contains(raw, "\\brepeat\\s+off\\b") || contains(raw, "\\bstop\\s+repeat(?:ing)?\\b") ||
            contains(raw, "\\bturn\\s+(?:off|of)\\s+repeat\\b") || contains(raw, "\\bdisable\\s+repeat\\b") ||
            contains(raw, "\\bstop\\s+looping\\b")
        ) {
            return Hit("SET_REPEAT", NluMutableSlots(repeatMode = NluRepeatMode.Off))
        }

        val wholeCollection = contains(raw, COLLECTION_RE)
        if (!wholeCollection) {
            if (contains(raw, "\\b(?:repeat|loop)\\s+(?:this(?:\\s+(?:song|track|one))?|current(?:\\s+(?:song|track))?|it)\\b") ||
                contains(raw, "\\bon\\s+repeat\\b") || contains(raw, "\\b(?:in|on)\\s+(?:a\\s+)?(?:repeat\\s+)?loop\\b") ||
                contains(raw, "\\bbe\\s+looped\\b") || contains(raw, "\\bloop\\s+(?:this|that|it)\\b") ||
                contains(raw, "\\bover\\s+and\\s+over\\b")
            ) {
                return Hit("SET_REPEAT", NluMutableSlots(repeatMode = NluRepeatMode.One))
            }
        }

        if (wholeCollection && contains(raw, "\\b(?:repeat|loop)\\b")) {
            return Hit("SET_REPEAT", NluMutableSlots(repeatMode = NluRepeatMode.All))
        }
        if (contains(raw, "\\brepeat(?:\\s+on)?\\s*$") || raw in listOf("repeat", "loop", "repeat on", "loop on") ||
            contains(raw, "\\bturn\\s+on\\s+repeat\\b") || contains(raw, "\\benable\\s+repeat\\b")
        ) {
            return Hit("SET_REPEAT", NluMutableSlots(repeatMode = NluRepeatMode.All))
        }
        return null
    }

    private fun ruleShuffle(tokens: List<String>, raw: String, core: String): Hit? {
        if ("shuffle" !in tokens && "shuffling" !in tokens && "mix" !in tokens && "randomize" !in tokens) {
            return null
        }
        if (("play" in tokens || "start" in tokens) && contains(raw, NAMED_COLLECTION_RE)) return null
        if (contains(raw, "\\bshuffle\\s+off\\b") || contains(raw, "\\bstop\\s+shuffling\\b") ||
            contains(raw, "\\bturn\\s+off\\s+shuffle\\b") || contains(raw, "\\bdisable\\s+shuffle\\b")
        ) {
            return Hit("SET_SHUFFLE", NluMutableSlots(enabled = false))
        }
        if (coreIsOnly(
                core,
                target = setOf("shuffle", "shuffling", "mix"),
                leads = setOf(
                    "on", "up", "turn", "enable", "start", "play", "and", "repeat", "put",
                    "mode", "needs", "randomize",
                ),
            )
        ) {
            return Hit("SET_SHUFFLE", NluMutableSlots(enabled = true))
        }
        return null
    }

    private fun ruleWhatsPlaying(tokens: List<String>, raw: String, core: String): Hit? {
        if (tokens.size > 8) return null
        val patterns = listOf(
            "^what'?s\\s+playing\\s*$",
            "^what'?s\\s+this(?:\\s+song)?\\s*$",
            "^what\\s+is\\s+this(?:\\s+song)?\\s*$",
            "^who'?s\\s+(?:this|playing)\\s*$",
            "^who\\s+is\\s+(?:this|playing)\\s*$",
            "\\bname\\s+of\\s+(?:the\\s+|this\\s+)?(?:song|track|artist)\\b",
            "^what\\s+song\\s+is\\s+this\\s*$",
        )
        for (p in patterns) {
            if (contains(raw, p)) return Hit("SHOW_VIEW", NluMutableSlots(view = NluView.NowPlaying))
        }
        return null
    }

    private fun ruleUnmute(tokens: List<String>, raw: String, core: String): Hit? {
        if (contains(raw, "\\bunmute\\b")) return Hit("SET_VOLUME", NluMutableSlots(mute = false))
        if (contains(raw, "\\bturn\\s+(?:the\\s+)?(?:sound|audio|volume)\\s+back\\s+on\\b")) {
            return Hit("SET_VOLUME", NluMutableSlots(mute = false))
        }
        return null
    }

    private fun ruleMute(tokens: List<String>, raw: String, core: String): Hit? {
        if (contains(raw, "^mute(?:\\s+(?:it|the\\s+(?:audio|sound|volume|music)))?\\s*$")) {
            return Hit("SET_VOLUME", NluMutableSlots(mute = true))
        }
        if (contains(raw, "\\bturn\\s+(?:off|down\\s+to\\s+zero)\\s+(?:the\\s+)?(?:sound|audio|volume)\\b")) {
            return Hit("SET_VOLUME", NluMutableSlots(mute = true))
        }
        return null
    }

    private fun amountModifier(raw: String): NluAmount {
        if (contains(raw, "\\ba\\s+(?:little|bit|tiny\\s+bit|touch)\\b")) return NluAmount.Small
        if (contains(raw, "\\ba\\s+lot\\b|\\bway\\b|\\bmuch\\s+(?:louder|higher|quieter|lower)\\b")) return NluAmount.Large
        return NluAmount.Medium
    }

    private fun ruleVolumeUp(tokens: List<String>, raw: String, core: String): Hit? {
        val patterns = listOf(
            "\\bvolume\\s+up\\b",
            "^louder\\s*$",
            "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*up\\b",
            "\\bturn\\s+up\\s+(?:the\\s+)?volume\\b",
            "\\bcrank\\s+(?:it\\s+)?up\\b",
            "^make\\s+(?:it\\s+)?louder\\s*$",
        )
        for (p in patterns) {
            if (contains(raw, p)) {
                return Hit("SET_VOLUME", NluMutableSlots(direction = NluDirection.Up, amount = amountModifier(raw)))
            }
        }
        return null
    }

    private fun ruleVolumeDown(tokens: List<String>, raw: String, core: String): Hit? {
        if (contains(raw, "\\bvolume\\s+down\\b") || raw in listOf("quieter", "softer") ||
            contains(raw, "\\bturn\\s+(?:it|the\\s+(?:volume|music))?\\s*down\\b") ||
            contains(raw, "\\bturn\\s+down\\s+(?:the\\s+)?volume\\b") ||
            contains(raw, "^make\\s+(?:it\\s+)?(?:quieter|softer)\\s*$")
        ) {
            return Hit("SET_VOLUME", NluMutableSlots(direction = NluDirection.Down, amount = amountModifier(raw)))
        }
        return null
    }

    private fun rulePlayResume(tokens: List<String>, raw: String, core: String): Hit? {
        if (coreIsOnly(core, target = setOf("resume"), leads = setOf("playing", "play"))) {
            return Hit("PLAY", NluMutableSlots())
        }
        if (core in listOf("keep playing", "keep going")) {
            return Hit("PLAY", NluMutableSlots())
        }
        return null
    }

    private fun rulePause(tokens: List<String>, raw: String, core: String): Hit? {
        if (coreIsOnly(core, target = setOf("pause"), leads = setOf("playing"))) {
            return Hit("PAUSE", NluMutableSlots())
        }
        return null
    }

    private fun rulePauseStop(tokens: List<String>, raw: String, core: String): Hit? {
        if ("repeat" in tokens || raw.contains("shuffl")) return null
        if (coreIsOnly(core, target = setOf("stop", "end"), leads = setOf("playing", "play", "from"))) {
            return Hit("PAUSE", NluMutableSlots())
        }
        if (coreIsOnly(core, target = setOf("off"), leads = setOf("turn", "playing", "play"))) {
            return Hit("PAUSE", NluMutableSlots())
        }
        return null
    }

    private fun ruleNext(tokens: List<String>, raw: String, core: String): Hit? {
        if (core.contains("back") || contains(raw, NAMED_COLLECTION_RE)) return null
        if (coreIsOnly(
                core,
                target = setOf("next", "skip"),
                leads = setOf("play", "go", "hear", "listen", "to", "one", "ahead", "forward"),
            )
        ) {
            return Hit("NEXT", NluMutableSlots())
        }
        return null
    }

    private val previousLeads = setOf(
        "play", "go", "hear", "listen", "to", "back", "one", "again", "more", "time",
        "start", "from", "beginning", "over",
    )

    private fun rulePrevious(tokens: List<String>, raw: String, core: String): Hit? {
        if (contains(raw, NAMED_COLLECTION_RE)) return null
        if (coreIsOnly(core, target = setOf("previous", "replay"), leads = previousLeads + "last")) {
            return Hit("PREVIOUS", NluMutableSlots())
        }
        if (coreIsOnly(core, target = setOf("last"), leads = previousLeads + setOf("repeat", "replay")) &&
            contains(core, "\\b(?:play|go|hear|listen|back|repeat|replay|start)\\b")
        ) {
            return Hit("PREVIOUS", NluMutableSlots())
        }
        if (coreIsOnly(core, target = setOf("back"), leads = setOf("go", "one", "to"))) {
            return Hit("PREVIOUS", NluMutableSlots())
        }
        return null
    }

    private fun rulePlayBare(tokens: List<String>, raw: String, core: String): Hit? {
        if (coreIsOnly(core, target = setOf("play", "start"), leads = setOf("playing", "something", "go", "on"))) {
            return Hit("PLAY", NluMutableSlots())
        }
        if (core == "go") return Hit("PLAY", NluMutableSlots())
        return null
    }

    private val rules: List<(List<String>, String, String) -> Hit?> = listOf(
        ::ruleSaveToPreset,
        ::rulePlayPreset,
        ::ruleVolumeAbsolute,
        ::rulePlaybackSpeed,
        ::ruleSeek,
        ::ruleRepeat,
        ::ruleShuffle,
        ::ruleWhatsPlaying,
        ::ruleUnmute,
        ::ruleMute,
        ::ruleVolumeUp,
        ::ruleVolumeDown,
        ::rulePlayResume,
        ::rulePause,
        ::rulePauseStop,
        ::ruleNext,
        ::rulePrevious,
        ::rulePlayBare,
    )
}
