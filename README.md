
# FL Caption
![fl_caption_resize](https://github.com/user-attachments/assets/1662d23c-0d58-413e-b57f-d16b92452f36)


[![Windows Build](https://github.com/xkeyC/fl_caption/actions/workflows/windows_build.yml/badge.svg)](https://github.com/xkeyC/fl_caption/actions/workflows/windows_build.yml)
[![Linux Build](https://github.com/xkeyC/fl_caption/actions/workflows/linux_build.yml/badge.svg)](https://github.com/xkeyC/fl_caption/actions/workflows/linux_build.yml)
[![macOS Build](https://github.com/xkeyC/fl_caption/actions/workflows/macos_build.yml/badge.svg)](https://github.com/xkeyC/fl_caption/actions/workflows/macos_build.yml)

Offline real-time captioning software built with Flutter and Rust, powered by LLM and Whisper (based on the candle inference framework / onnx).

Demo video: https://www.bilibili.com/video/BV1VyQtYMEWA

QQ Group: 1037016702

![image.png](https://s2.loli.net/2025/03/15/5PbgI1WYapKt4jR.png)


## Usage

1. Download the archive from the [Releases](https://github.com/xkeyC/fl_caption/releases) page and extract it.

2. On first use, click the settings icon, select an appropriate speech model, and click the download button.

3. After downloading, select the audio language and caption language, configure the LLM API info, then click Save.

4. Captions should start running normally.

## FAQ

1. Model download stuck or failed: Try setting the `HF_ENDPOINT` environment variable as described in https://github.com/xkeyC/fl_caption/issues/1, or manually download the files by opening the link in https://github.com/xkeyC/fl_caption/blob/main/lib/common/whisper/models.dart — the file name is the value of `name`, e.g., `base`, `large-v3_q4k`, etc.

2. Stuck on "Wait for Whisper" after startup: If CUDA acceleration is enabled, make sure the CUDA Toolkit is installed (download: https://developer.nvidia.com/cuda-downloads?target_os=Windows). If it is not a CUDA issue, please open an issue and provide your hardware specs.
    > Tip: When installing, select only Development and Runtime -> Libraries to optimize installation speed and file size.
    ![image.png](https://s2.loli.net/2025/03/16/dZiXMquhF1YDj2U.png)

## Acknowledgments
This project references several open-source projects for audio inference, thanks to:

- [candle](https://github.com/huggingface/candle)
- [ort](https://github.com/pykeio/ort)
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
- [sensevoice-rs](https://github.com/darkautism/sensevoice-rs)
- [Olive](https://github.com/microsoft/Olive)