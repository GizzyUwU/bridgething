import BridgethingSchema
import Foundation

enum VoiceOpusFixture {
    static let sampleRateHz: UInt32 = 16000
    static let toneHz = 440.0
    static let samplesPerPacket = 320

    static let format = VoiceFormat(codec: .opus, sampleRateHz: sampleRateHz, channels: 1)

    static var packets: [Data] { encoded.map { Data(base64Encoded: $0)! } }

    private static let encoded = [
        "SIM+0FQqCLwAACZLL7v4FFpr7o+BpsEja0HKOJ8EnJlg+0UBFbx2iYX0DD/RKEMVN/xosoun3PUTt8Fi5MDrMm/w30HOOYqVNY/YohCYpNbgZgIhAgqM",
        "SKp+n1feghIqpaEf9a/6LHkMvcqRX4HWfJy0xwPaR1+9H9bRr2tR53dYmsAakaA2oA==",
        "SJ/6jAm0IA6JzCrgOOid8HBHLBq8K+smMPdHXwmFYxW154jwP35Ev9qA",
        "SJ/6jAm0H/+7AL1sI2+DP6RfqcRiqMBiRbMbxglFWT6UkDQVAECLhSXpCXq8Hw==",
        "SJ/6jAm0IAkClieTTQG4xqqrVLHkCwrfAOwP6xKvlGoUwRDGoJcC1w3kxiVcuWuapWBSNpG2",
        "SJ+VtrhIjd2FnM4DlLRKZkbZNFj0nKXydLgq+QQEsb5z0sMRkSZce4CNzbLdmpVld6Xei+XmQA==",
        "SJ6yLAm0IAATrUG7R9MurwxPj4wganKXv6LpaYS1sNzT0ale4Uyl9VkB+hkxIXAI1RWu2qEQNmjKtA==",
        "SJ6yLAm0IA7TLyathxFaoKZUiqFtxtP4Yz/JuWcCsatdhD/oSHpCa27CbG/EHBtmcA==",
        "SJ6yLBBZ0mE/9xEP7t5nhRSEYXVT7IG0kcn8XZOhDn/X+upcHtOz6/SIDlyyPaYKCRhA",
        "SJ6yLBBZ0mECuoy/UUsohFKtz7e1DIi8JKjx1smbcgTi1z8PoPL2Tqw9prda7tuNiBje//2NoMjb0A==",
        "SJ6yLBBZ0maHNXYqIgsQU698KAqxfHuFMppMDjzt9c4XwIQs30JMsaJmSx0rgQAo",
        "SJ6yLAm0IAAYAPEe/6QnU5nUsIUdGhnxRrEmEnfXKmPnejXehSeSBVEtS+gOw/kX7wJw72b4CzQ=",
        "SJ6yLAm0IA7TLhyOYX6s8W6yLB3oYfSLJzsdQaWod2Npp4cLWYD4DGkkKyTdnq54iTuA",
        "SJ6yLAm0H/+7AovpfXjIVZ4VaIy1kbRKKCYIVG/2zI+P5hNUh0VIDAAGtvQNu9Q=",
        "SJ6yLAm0IAkCuw72w7EqCVBrMb47G+BxQ1VX3srpy7uJdnslZOL4/IHzp28pwI7byTjl5enM",
    ]

    static func energy(of pcm: [Int16], atHz hz: Double) -> Double {
        let coeff = 2 * cos(2 * Double.pi * hz / Double(sampleRateHz))
        var s1 = 0.0
        var s2 = 0.0
        for sample in pcm {
            let s = Double(sample) / 32768 + coeff * s1 - s2
            s2 = s1
            s1 = s
        }
        return s1 * s1 + s2 * s2 - coeff * s1 * s2
    }

    static func samples(inWav bytes: Data) -> [Int16] {
        let body = bytes.dropFirst(44)
        return stride(from: body.startIndex, to: body.endIndex - 1, by: 2).map {
            Int16(bitPattern: UInt16(body[$0]) | UInt16(body[$0 + 1]) << 8)
        }
    }
}
