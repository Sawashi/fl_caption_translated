import 'package:fluent_ui/fluent_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'settings_provider.dart';

class SettingsInferencePage extends HookWidget {
  final ValueNotifier<AppSettingsData?> appSettingsData;
  final TextEditingController whisperMaxAudioDurationController;
  final TextEditingController inferenceIntervalController;
  final TextEditingController whisperDefaultMaxDecodeTokensController;
  final TextEditingController whisperTemperatureController;
  final TextEditingController llmTemperatureController;
  final TextEditingController llmMaxTokensController;
  final TextEditingController llmPromptPrefixController;

  const SettingsInferencePage({
    super.key,
    required this.appSettingsData,
    required this.whisperMaxAudioDurationController,
    required this.inferenceIntervalController,
    required this.whisperDefaultMaxDecodeTokensController,
    required this.whisperTemperatureController,
    required this.llmTemperatureController,
    required this.llmMaxTokensController,
    required this.llmPromptPrefixController,
  });

  @override
  Widget build(BuildContext context) {
    if (appSettingsData.value == null) {
      return const Center(child: ProgressRing());
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('Inference Settings', style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold)),
        const SizedBox(height: 24),

        // Whisper settings
        const Text('Whisper', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
        const SizedBox(height: 16),

        _buildSettingRow(
          label: 'Audio Length (seconds):',
          tooltip: 'Longer audio increases inference load. If speech is fast, reducing the length can help reduce load. (Default: 12s)',
          controller: whisperMaxAudioDurationController,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        ),

        _buildSettingRow(
          label: 'Inference Interval (ms):',
          tooltip: 'Lower intervals reduce latency, but should not be less than the GPU inference time or it may cause OOM. (Default: 2000ms)',
          controller: inferenceIntervalController,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        ),

        _buildSettingRow(
          label: 'Max Decode Tokens:',
          tooltip: 'Limiting this value prevents Whisper from getting stuck in hallucination loops for too long. (Default: 256)',
          controller: whisperDefaultMaxDecodeTokensController,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        ),

        _buildSettingRow(
          label: 'Whisper Temperature:',
          tooltip: 'Lower values make output more deterministic, higher values make output more creative. (0.0-1.0, Default: 0.0)',
          controller: whisperTemperatureController,
          inputFormatters: [
            FilteringTextInputFormatter.allow(RegExp(r'^\d*\.?\d*$')),
            _TemperatureTextInputFormatter(),
          ],
        ),

        const SizedBox(height: 24),

        // LLM settings
        const Text('LLM', style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
        const SizedBox(height: 16),

        _buildSettingRow(
          label: "LLM Prompt Prefix",
          tooltip: "Add a prefix to the LLM prompt. For example, for the qwen3 model, add a /no_think prefix to disable model thinking.",
          controller: llmPromptPrefixController,
        ),

        _buildSettingRow(
          label: 'LLM Temperature:',
          tooltip: 'Lower values make output more deterministic, higher values make output more creative. (0.0-1.0, Default: 0.1)',
          controller: llmTemperatureController,
          inputFormatters: [
            FilteringTextInputFormatter.allow(RegExp(r'^\d*\.?\d*$')),
            _TemperatureTextInputFormatter(),
          ],
        ),

        _buildSettingRow(
          label: 'LLM Max Output Tokens:',
          tooltip: 'Limit the maximum output length of the LLM. (Default: 256)',
          controller: llmMaxTokensController,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
        ),
      ],
    );
  }

  Widget _buildSettingRow({
    required String label,
    required String tooltip,
    required TextEditingController controller,
    List<TextInputFormatter>? inputFormatters,
  }) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 16.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              SizedBox(width: 180, child: Text(label)),
              Expanded(child: TextFormBox(controller: controller, inputFormatters: inputFormatters)),
            ],
          ),
          SizedBox(height: 4),
          SelectionArea(child: Text(tooltip, style: TextStyle(color: Colors.white.withValues(alpha: .6)))),
        ],
      ),
    );
  }
}

class _TemperatureTextInputFormatter extends TextInputFormatter {
  @override
  TextEditingValue formatEditUpdate(TextEditingValue oldValue, TextEditingValue newValue) {
    if (newValue.text.isEmpty) {
      return newValue;
    }
    // Parse float value
    double? value = double.tryParse(newValue.text);
    if (value == null) {
      return oldValue;
    }

    // Ensure value is between 0.0 and 1.0
    if (value < 0.0) {
      return const TextEditingValue(text: "0.0");
    } else if (value > 1.0) {
      return const TextEditingValue(text: "1.0");
    }

    return newValue;
  }
}
