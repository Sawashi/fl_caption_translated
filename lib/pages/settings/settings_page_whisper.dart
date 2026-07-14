import 'dart:io';

import 'package:fl_caption/common/dialog_utils.dart';
import 'package:fl_caption/common/whisper/models.dart';
import 'package:fl_caption/common/whisper/onnx_models.dart';
import 'package:fl_caption/dialogs/model_download_dialog.dart';
import 'package:fl_caption/dialogs/model_download_provider.dart';
import 'package:fl_caption/pages/settings/settings_provider.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

class SettingsWhisperPage extends HookConsumerWidget {
  final ValueNotifier<AppSettingsData?> appSettingsData;
  final TextEditingController modelDirController;

  const SettingsWhisperPage({super.key, required this.appSettingsData, required this.modelDirController});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text("Whisper", style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
        const SizedBox(height: 16),
        // CUDA toggle
        ToggleSwitch(
          checked: appSettingsData.value?.tryWithCuda ?? true,
          onChanged: (value) {
            appSettingsData.value = appSettingsData.value?.copyWith(tryWithCuda: value);
          },
          content: Text(Platform.isMacOS ? "Enable Metal Acceleration (requires Apple Silicon)" : "Enable GPU Acceleration"),
        ),
        const SizedBox(height: 16),
        _buildModelFolderSection(modelDirController),
        const SizedBox(height: 16),
        _buildModelSelectionSection(appSettingsData, ref, modelDirController),
        const SizedBox(height: 16),
        ToggleSwitch(
          checked: appSettingsData.value?.withVAD ?? true,
          onChanged: (value) {
            appSettingsData.value = appSettingsData.value?.copyWith(withVAD: value);
          },
          content: const Text("Use VAD Model (reduces hallucinations from non-speech audio, adds a small amount of inference time)"),
        ),
        const SizedBox(height: 16),
        if (appSettingsData.value?.withVAD ?? true) ...[
          Row(
            children: [
              Text("VAD Threshold:"),
              const SizedBox(width: 8),
              // slider 0 ~ 1  step 0.1
              SizedBox(
                width: 400,
                child: Slider(
                  value: appSettingsData.value?.vadThreshold ?? 0.1,
                  min: 0,
                  max: 1,
                  divisions: 100,
                  label: (appSettingsData.value?.vadThreshold ?? 0.1).toStringAsFixed(2),
                  onChanged: (value) {
                    appSettingsData.value = appSettingsData.value?.copyWith(vadThreshold: value);
                  },
                ),
              ),
              Text(" ${(appSettingsData.value?.vadThreshold ?? 0.1).toStringAsFixed(2)}"),
            ],
          ),
          SizedBox(height: 6),
          Text(
            "VAD model scores the audio; audio below this threshold will be filtered. Range: 0.0 ~ 1.0, Default: 0.1",
            style: TextStyle(fontSize: 14, color: Colors.white.withValues(alpha: .6)),
          ),
        ],
      ],
    );
  }

  Widget _buildModelFolderSection(TextEditingController modelDirController) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InfoLabel(label: "Model Folder Path:"),
        Row(
          children: [
            Expanded(child: TextBox(controller: modelDirController, placeholder: "Select model folder path")),
            const SizedBox(width: 8),
            IconButton(
              icon: const Icon(FluentIcons.folder_search),
              onPressed: () async {
                // TODO pick folder use main window
                // final path = await FilePicker.platform.getDirectoryPath(
                //   lockParentWindow: true,
                //   initialDirectory: modelDirController.text.trim(),
                //   dialogTitle: "Select folder path",
                // );
                // if (path != null) {
                //   modelDirController.text = path;
                // }
              },
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildModelSelectionSection(
    ValueNotifier<AppSettingsData?> appSettingsData,
    WidgetRef ref,
    TextEditingController modelDirController,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InfoLabel(label: "Speech Recognition Model:"),
        Row(
          children: [
            Expanded(
              child: ComboBox<String>(
                placeholder: const Text('Select Model'),
                isExpanded: true,
                value: appSettingsData.value?.whisperModel,
                items:
                    whisperModels.values
                        .map(
                          (model) => ComboBoxItem<String>(
                            value: model.name,
                            child: Text(model is OnnxModelsData ? "[ONNX] ${model.name}" : model.name),
                          ),
                        )
                        .toList(),
                onChanged: (value) {
                  if (value != null) {
                    appSettingsData.value = appSettingsData.value?.copyWith(whisperModel: value);
                  }
                },
              ),
            ),
            const SizedBox(width: 8),
            _buildModelDownloadButton(appSettingsData, ref, modelDirController),
          ],
        ),
      ],
    );
  }

  Widget _buildModelDownloadButton(
    ValueNotifier<AppSettingsData?> appSettingsData,
    WidgetRef ref,
    TextEditingController modelDirController,
  ) {
    return Consumer(
      builder: (BuildContext context, WidgetRef ref, Widget? child) {
        final modelState = ref.watch(
          modelDownloadStateProvider(appSettingsData.value?.whisperModel ?? "", modelDirController.text),
        );
        if (modelState.isReady) {
          return Icon(FluentIcons.check_mark);
        }
        return IconButton(
          icon: const Icon(FluentIcons.download),
          onPressed: () async {
            final modelName = appSettingsData.value!.whisperModel;
            final modelData = whisperModels[modelName];
            final ok = await showConfirmDialogs(context, "Confirm download model $modelName?", Text("This will take approximately ${modelData?.size} of space"));
            var savePath = modelDirController.text.trim();
            if (ok) {
              if (!context.mounted) return;
              final downloadOK = await showDialog(
                context: context,
                builder: (BuildContext context) {
                  return ModelDownloadDialog(model: modelData!, savePath: savePath);
                },
              );
              if (downloadOK != true) {
                if (!context.mounted) return;
                showToast(context, "Download failed: ${modelState.errorText}");
              }
            }
          },
        );
      },
    );
  }
}
