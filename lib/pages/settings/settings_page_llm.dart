import 'package:fl_caption/pages/settings/settings_provider.dart';
import 'package:fluent_ui/fluent_ui.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

class SettingsLlmPage extends HookConsumerWidget {
  final TextEditingController apiUrlController;
  final TextEditingController apiKeyController;
  final TextEditingController apiModelController;
  final ValueNotifier<AppSettingsData?> appSettingsData;

  const SettingsLlmPage({
    super.key,
    required this.appSettingsData,
    required this.apiUrlController,
    required this.apiKeyController,
    required this.apiModelController,
  });

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text("LLM", style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
        const SizedBox(height: 16),
        InfoLabel(label: "API Endpoint"),
        TextBox(
          controller: apiUrlController,
          placeholder: "Enter the full completions URL, e.g.: http://localhost:11434/v1/chat/completions",
        ),
        const SizedBox(height: 16),
        InfoLabel(label: "API Key"),
        TextBox(controller: apiKeyController, placeholder: "Enter API key (Ollama default is empty)", obscureText: true),
        const SizedBox(height: 16),
        InfoLabel(label: "Model Name"),
        TextBox(controller: apiModelController, placeholder: "e.g.: phi4:14b"),
        const SizedBox(height: 16),
        // llm_context_optimization
        ToggleSwitch(
          checked: appSettingsData.value?.llmContextOptimization ?? true,
          onChanged: (value) {
            appSettingsData.value = appSettingsData.value?.copyWith(llmContextOptimization: value);
          },
          content: const Text("Enable context optimization (uses more tokens)"),
        ),
      ],
    );
  }
}
