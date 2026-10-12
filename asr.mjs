// Whisper speech-to-text with "context prompting": what Claude just said is fed in as prior text,
// so names and topics from the conversation (e.g. "Ada Wong") are heard correctly.
import { AutoProcessor, AutoTokenizer, WhisperForConditionalGeneration } from '@huggingface/transformers';

const MODEL = process.env.CALLME_ASR || 'onnx-community/whisper-small.en';
const [processor, tokenizer, model] = await Promise.all([
  AutoProcessor.from_pretrained(MODEL),
  AutoTokenizer.from_pretrained(MODEL),
  WhisperForConditionalGeneration.from_pretrained(MODEL, { dtype: 'fp32', device: 'cpu' }),
]);
const [PREV, SOT, NO_TS] = tokenizer.model.convert_tokens_to_ids(['<|startofprev|>', '<|startoftranscript|>', '<|notimestamps|>']);

export async function transcribe(pcm, context = '') {
  const { input_features } = await processor(pcm);
  // Whisper allows up to 224 prompt tokens; keep the most recent context.
  const prompt = context ? tokenizer.encode(' ' + context.trim(), { add_special_tokens: false }).slice(-200) : [];
  const init = prompt.length ? [PREV, ...prompt, SOT, NO_TS] : [SOT, NO_TS];
  const out = await model.generate({ inputs: input_features, decoder_input_ids: [init], max_new_tokens: 220 });
  const ids = Array.from(out.tolist()[0], Number).slice(init.length);
  return tokenizer.decode(ids, { skip_special_tokens: true }).trim();
}
