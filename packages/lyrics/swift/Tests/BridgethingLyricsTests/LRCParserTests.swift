import Testing
@testable import BridgethingLyrics

@Test func parsesSingleTimestampLines() {
    let input = """
    [00:12.50]Line one
    [00:17.00]Line two
    [01:30.50]Line three
    """
    let lines = LRCParser.parse(input)
    #expect(lines.count == 3)
    #expect(lines[0] == LyricLine(startMs: 12500, text: "Line one"))
    #expect(lines[1] == LyricLine(startMs: 17000, text: "Line two"))
    #expect(lines[2] == LyricLine(startMs: 90500, text: "Line three"))
}

@Test func expandsMultipleTimestampsOnOneLine() {
    let input = "[00:12.00][01:30.00]Repeated chorus"
    let lines = LRCParser.parse(input)
    #expect(lines.count == 2)
    #expect(lines[0] == LyricLine(startMs: 12000, text: "Repeated chorus"))
    #expect(lines[1] == LyricLine(startMs: 90000, text: "Repeated chorus"))
}

@Test func skipsLinesWithoutTimestamps() {
    let input = """
    [ti: Title]
    [ar: Artist]
    [00:12.50]Line one
    """
    let lines = LRCParser.parse(input)
    #expect(lines.count == 1)
    #expect(lines[0].text == "Line one")
}

@Test func acceptsThreeDigitFractionalSeconds() {
    let lines = LRCParser.parse("[00:12.500]Line")
    #expect(lines.count == 1)
    #expect(lines[0].startMs == 12500)
}

@Test func emitsSortedByTimestamp() {
    let lines = LRCParser.parse("[01:30.00]B\n[00:12.50]A")
    #expect(lines[0].text == "A")
    #expect(lines[1].text == "B")
}
