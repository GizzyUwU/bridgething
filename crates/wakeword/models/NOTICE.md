# Wake-word model provenance and licensing

These files are **not** covered by the repository's MIT license. They are third-party artifacts,
plus one artifact of ours trained against them, and they carry a different and more restrictive
license. Read this before redistributing anything in this directory.

## Files

| File | Origin | License | Ships on device |
|---|---|---|---|
| `melspectrogram.onnx` | [openWakeWord](https://github.com/dscripka/openWakeWord) | CC BY-NC-SA 4.0 | yes |
| `embedding_model.onnx` | [openWakeWord](https://github.com/dscripka/openWakeWord) | CC BY-NC-SA 4.0 | yes |
| `hey_bridgething.onnx` | Ours, trained with openWakeWord | CC BY-NC-SA 4.0 (see below) | yes |
| `hey_jarvis_v0.1.onnx` | [openWakeWord](https://github.com/dscripka/openWakeWord) | CC BY-NC-SA 4.0 | no, test control only |

`hey_jarvis_v0.1.onnx` is upstream's published model, kept as the known-good control the golden tests
score against. It never reaches a device, but it is redistributed by this repository and so carries
the same obligations.

`SPDX-License-Identifier: CC-BY-NC-SA-4.0`

Full license text: <https://creativecommons.org/licenses/by-nc-sa/4.0/legalcode>

## Attribution

openWakeWord is by David Scripka. Its **code** is Apache 2.0; its **pre-trained models** are
CC BY-NC-SA 4.0, which upstream attributes to "the inclusion of datasets with unknown or restrictive
licensing as part of the training data."

`embedding_model.onnx` is openWakeWord's re-implementation of Google's
[speech embedding model](https://tfhub.dev/google/speech_embedding/1), which is itself Apache 2.0 as
a TFHub module. The re-implementation and retraining is what carries the Creative Commons license,
so the permissive upstream does not flow through.

The three openWakeWord files are redistributed unmodified.

## Why our own model is marked the same

`hey_bridgething.onnx` contains none of openWakeWord's weights. It is trained on our own
piper-generated speech, passed through `embedding_model.onnx` as a feature extractor, using
openWakeWord's Apache-2.0 training code.

**ShareAlike very probably does not reach it, and the marking is not a guess about that.** The
question of whether a trained model is "Adapted Material" has been analysed at length and never
litigated. Creative Commons declines to take a position and frames its own guidance as "the most
restrictive legal interpretation for those who wish to take a conservative approach". The most
thorough treatment, Szkalej and Senftleben's study for IViR (2024), concludes that the concepts CC
would have to lean on, "adapted material" and "technical modification", do not cleanly support
attaching ShareAlike to trained models at all, and that where text-and-data-mining exceptions prevail
it is "particularly difficult, if not impossible" to do so. Extending ShareAlike to models would take
a purpose-built licence rather than the existing CC ones.

Our case sits further from that edge again: the training data is ours, and their model is used as a
tool over it rather than being trained on.

It is marked CC BY-NC-SA 4.0 anyway, for a plainer reason than legal caution. The classifier is inert
without `embedding_model.onnx`, which is unambiguously NC. Marking it permissively would advertise a
freedom nobody actually has, since anyone using it needs the NC model too.

## Consequences

- **NonCommercial.** No artifact containing these files may be distributed commercially. That
  includes any device image that ships them. This is consistent with the project's stated posture,
  where non-commercial use is the point rather than a limitation.
- **ShareAlike.** Distributing an adaptation of these models requires the same license.
- **Attribution.** This file must travel with the models wherever they go, including into an image.
- **Bundling is a collection, not an adaptation.** Shipping these alongside MIT-licensed daemon code
  does not relicense that code. The rest of the repository stays MIT.

## For a fork that wants to go commercial

Every file here has to be replaced or dropped. Only one of them is genuinely hard:

- `melspectrogram.onnx` is an ONNX export of a mel spectrogram, not a trained model. It can be
  replaced with native code, which is an STFT and a mel filterbank against an FFT that `crates/dsp`
  already has.
- `hey_bridgething.onnx` can be retrained once a permissive backbone exists.
- `hey_jarvis_v0.1.onnx` can simply be dropped, along with the golden tests that use it as a control.
- `embedding_model.onnx` is the hard one. It carries the pre-training that makes the whole approach
  work and cannot be reimplemented, only substituted. Upstream invites requests for more permissively
  licensed pre-trained models, which is the cheapest route if it ever matters.
