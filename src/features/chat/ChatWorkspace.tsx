import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  AlertTriangle,
  ArrowUp,
  Bot,
  Download,
  ImagePlus,
  LoaderCircle,
  Plus,
  Search,
  SlidersHorizontal,
  Sparkles,
  Square,
  User,
  X,
} from "lucide-react";
import { bridge } from "../../services/bridge";
import { useAppStore } from "../../stores/app-store";
import type {
  ChatGenerationState,
  ChatMessageInput,
  ChatUsage,
  EngineRuntimeStatus,
  ModelRecord,
} from "../../types/domain";
import {
  chatModelLabel,
  groupChatModels,
  isEngineActive,
  isSelectedModelReady,
} from "./model-selection";

const LAST_MODEL_KEY = "neuraloc.lastModelId";
const AUTO_SCROLL_THRESHOLD_PX = 48;

type MessageState = "pending" | "streaming" | "complete" | "cancelled" | "error";

interface LocalMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  state: MessageState;
  usage: ChatUsage | null;
}

export function ChatWorkspace() {
  const setActiveView = useAppStore((state) => state.setActiveView);
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [runtimeStatus, setRuntimeStatus] = useState<EngineRuntimeStatus | null>(null);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [modelOperation, setModelOperation] = useState<"load" | "stop" | null>(null);
  const [conversationId, setConversationId] = useState(() => crypto.randomUUID());
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [input, setInput] = useState("");
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const tokenSequences = useRef(new Map<string, number>());
  const stateSequences = useRef(new Map<string, number>());
  const usageSequences = useRef(new Map<string, number>());
  const messageViewport = useRef<HTMLDivElement | null>(null);
  const autoScrollToBottom = useRef(true);

  const loadWorkspace = useCallback(async () => {
    setLoading(true);
    try {
      const [availableModels, status] = await Promise.all([
        bridge.listModels(),
        bridge.getEngineStatus(),
      ]);
      setModels(availableModels);
      setRuntimeStatus(status);
      const activeModel = status.modelId && isEngineActive(status)
        && availableModels.some((model) => model.id === status.modelId && model.verificationState === "ready")
        ? status.modelId
        : null;
      const storedModel = readLastModelId();
      const rememberedModel = storedModel
        && availableModels.some((model) => model.id === storedModel && model.verificationState === "ready")
        ? storedModel
        : null;
      setSelectedModelId(activeModel ?? rememberedModel);
    } catch (caught) {
      setError(errorMessage(caught, "Chat could not load the local model library."));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadWorkspace();
  }, [loadWorkspace]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const keep = (unlisten: () => void) => disposed ? unlisten() : unlisteners.push(unlisten);

    void bridge.onEngineStateChanged((status) => {
      setRuntimeStatus(status);
      if (status.modelId && ["starting", "loadingModel", "ready", "busy"].includes(status.lifecycle)) {
        setSelectedModelId(status.modelId);
        writeLastModelId(status.modelId);
      }
    }).then(keep);
    void bridge.onChatToken((batch, sequence) => {
      const previous = tokenSequences.current.get(batch.jobId) ?? 0;
      if (sequence <= previous) return;
      tokenSequences.current.set(batch.jobId, sequence);
      setMessages((current) => current.map((message) => message.id === batch.messageId
        ? {
            ...message,
            content: message.content + batch.text,
            state: ["complete", "cancelled", "error"].includes(message.state) ? message.state : "streaming",
          }
        : message));
    }).then(keep);
    void bridge.onChatStateChanged((event, sequence) => {
      const previous = stateSequences.current.get(event.jobId) ?? 0;
      if (sequence <= previous) return;
      stateSequences.current.set(event.jobId, sequence);
      setMessages((current) => current.map((message) => message.id === event.messageId
        ? { ...message, state: messageState(event.state) }
        : message));
      if (event.state === "failed" && event.error) setError(event.error);
    }).then(keep);
    void bridge.onChatUsage((event, sequence) => {
      const previous = usageSequences.current.get(event.jobId) ?? 0;
      if (sequence <= previous) return;
      usageSequences.current.set(event.jobId, sequence);
      setMessages((current) => current.map((message) => message.id === event.messageId
        ? { ...message, usage: event.usage }
        : message));
    }).then(keep);

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (modelOperation !== "load") return;
    const timer = window.setInterval(() => {
      void bridge.getEngineStatus().then(setRuntimeStatus).catch(() => undefined);
    }, 500);
    return () => window.clearInterval(timer);
  }, [modelOperation]);

  useEffect(() => {
    const viewport = messageViewport.current;
    if (viewport && autoScrollToBottom.current) viewport.scrollTop = viewport.scrollHeight;
  }, [messages]);

  const groups = useMemo(() => groupChatModels(models), [models]);
  const selectedModel = models.find((model) => model.id === selectedModelId) ?? null;
  const modelReady = isSelectedModelReady(selectedModelId, runtimeStatus);
  const generating = activeJobId !== null;
  const runtimeAvailable = runtimeStatus?.lifecycle !== "notInstalled";

  async function selectModel(value: string) {
    if (value === "manage") {
      setActiveView("models");
      return;
    }
    setError(null);
    if (value === "none") {
      setSelectedModelId(null);
      writeLastModelId(null);
      if (runtimeStatus?.sessionId && isEngineActive(runtimeStatus)) await stopModel();
      return;
    }
    const model = models.find((candidate) => candidate.id === value && candidate.verificationState === "ready");
    if (!model) return;
    setSelectedModelId(model.id);
    writeLastModelId(model.id);
    await loadModel(model);
  }

  async function loadModel(model: ModelRecord) {
    setError(null);
    setModelOperation("load");
    try {
      let status = await bridge.getEngineStatus();
      setRuntimeStatus(status);
      if (status.lifecycle === "ready" && status.modelId === model.id) return;
      if (status.sessionId && isEngineActive(status)) {
        status = await bridge.stopEngine(status.sessionId);
        setRuntimeStatus(status);
      }
      status = await bridge.startEngine(model.id);
      setRuntimeStatus(status);
    } catch (caught) {
      try {
        setRuntimeStatus(await bridge.getEngineStatus());
      } catch {
        // Preserve the original loading error when status refresh also fails.
      }
      setError(errorMessage(caught, `${model.displayName} could not be loaded.`));
    } finally {
      setModelOperation(null);
    }
  }

  async function stopModel() {
    if (!runtimeStatus?.sessionId) return;
    setError(null);
    setModelOperation("stop");
    try {
      setRuntimeStatus(await bridge.stopEngine(runtimeStatus.sessionId));
    } catch (caught) {
      setError(errorMessage(caught, "The loaded model could not be stopped."));
    } finally {
      setModelOperation(null);
    }
  }

  async function sendMessage() {
    const content = input.trim();
    if (!content || !modelReady || !runtimeStatus?.sessionId || generating) return;
    setError(null);
    const jobId = crypto.randomUUID();
    const assistantMessageId = crypto.randomUUID();
    const userMessage: LocalMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content,
      state: "complete",
      usage: null,
    };
    const assistantMessage: LocalMessage = {
      id: assistantMessageId,
      role: "assistant",
      content: "",
      state: "pending",
      usage: null,
    };
    const history: ChatMessageInput[] = messages
      .filter((message) => message.role === "user" || message.state === "complete")
      .filter((message) => message.content.length > 0)
      .map((message) => ({ role: message.role, content: message.content }));
    const requestMessages = [...history, { role: "user" as const, content }];
    autoScrollToBottom.current = true;
    setMessages((current) => [...current, userMessage, assistantMessage]);
    setInput("");
    setActiveJobId(jobId);
    tokenSequences.current.delete(jobId);
    stateSequences.current.delete(jobId);
    usageSequences.current.delete(jobId);
    try {
      const result = await bridge.startChatGeneration({
        jobId,
        conversationId,
        messageId: assistantMessageId,
        sessionId: runtimeStatus.sessionId,
        messages: requestMessages,
        maxOutputTokens: 1024,
      });
      setMessages((current) => current.map((message) => message.id === assistantMessageId
        ? {
            ...message,
            state: messageState(result.state),
            usage: result.usage ?? message.usage,
          }
        : message));
    } catch (caught) {
      setMessages((current) => current.map((message) => message.id === assistantMessageId
        ? { ...message, state: "error" }
        : message));
      setError(errorMessage(caught, "The local model could not generate a response."));
    } finally {
      setActiveJobId(null);
      void bridge.getEngineStatus().then(setRuntimeStatus).catch(() => undefined);
    }
  }

  async function cancelGeneration() {
    if (!activeJobId) return;
    try {
      await bridge.cancelChatGeneration(activeJobId);
    } catch (caught) {
      setError(errorMessage(caught, "Generation could not be stopped."));
    }
  }

  async function newConversation() {
    if (activeJobId) await cancelGeneration();
    setConversationId(crypto.randomUUID());
    autoScrollToBottom.current = true;
    setMessages([]);
    setInput("");
    setError(null);
  }

  function handleComposerKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void sendMessage();
    }
  }

  function handleMessageScroll() {
    const viewport = messageViewport.current;
    if (!viewport) return;
    const distanceFromBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    autoScrollToBottom.current = distanceFromBottom <= AUTO_SCROLL_THRESHOLD_PX;
  }

  const statusCopy = chatStatus(runtimeStatus, selectedModel);
  const composerPlaceholder = modelReady
    ? `Message ${selectedModel?.displayName ?? "local model"}`
    : modelOperation === "load"
      ? `Loading ${selectedModel?.displayName ?? "model"}...`
      : "Select and load a model to begin";

  return <div className="chat-layout">
    <aside className="conversation-rail">
      <button className="new-chat-button" onClick={() => void newConversation()} type="button"><Plus size={17} /> New conversation</button>
      <div className="rail-search"><Search size={15} /><input aria-label="Search conversations" disabled placeholder="Search conversations" /></div>
      <div className="conversation-list">
        {messages.length > 0
          ? <button className="conversation-item active" type="button"><span>Current conversation</span><small>{selectedModel?.displayName ?? "Local chat"}</small></button>
          : <div className="conversation-empty">No conversations yet</div>}
      </div>
    </aside>
    <div className="chat-workspace">
      <div className="chat-controls">
        <label>Model<select
          aria-label="Chat model"
          disabled={loading || modelOperation !== null || generating}
          onChange={(event) => void selectModel(event.target.value)}
          value={selectedModelId ?? "none"}
        >
          <option value="none">No model selected</option>
          {groups.ready.length > 0 && <optgroup label={runtimeAvailable ? "Ready" : "Missing backend"}>
            {groups.ready.map((model) => <option disabled={!runtimeAvailable} key={model.id} value={model.id}>{chatModelLabel(model)}</option>)}
          </optgroup>}
          {groups.unavailable.length > 0 && <optgroup label="Unavailable">
            {groups.unavailable.map((model) => <option disabled key={model.id} value={model.id}>{model.displayName} - {model.verificationState}</option>)}
          </optgroup>}
          <option value="manage">Manage models...</option>
        </select></label>
        <label>System prompt<select defaultValue="default" onChange={(event) => { if (event.target.value === "manage") setActiveView("prompts"); }}><option value="default">No custom prompt</option><option value="manage">Manage prompt library...</option></select></label>
        <div className={`chat-runtime-state ${runtimeStatus?.lifecycle ?? "installed"}`}>
          {modelOperation === "load" || loading ? <LoaderCircle className="spin" size={14} /> : <span />}
          <small>{statusCopy}</small>
        </div>
        {activeJobId
          ? <button className="icon-button danger-action" onClick={() => void cancelGeneration()} title="Stop generation" type="button"><Square size={15} /></button>
          : runtimeStatus?.sessionId && isEngineActive(runtimeStatus)
            ? <button className="icon-button danger-action" disabled={modelOperation === "stop"} onClick={() => void stopModel()} title={modelOperation === "load" ? "Cancel model loading" : "Unload model"} type="button">{modelOperation === "stop" ? <LoaderCircle className="spin" size={16} /> : <Square size={15} />}</button>
            : <button className="icon-button" title="Generation settings" type="button"><SlidersHorizontal size={18} /></button>}
      </div>

      <div className="chat-stage">
      {error && <div className="error-banner chat-error"><AlertTriangle size={17} /><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError(null)} type="button"><X size={15} /></button></div>}

      {messages.length === 0 ? <div className="chat-empty">
        <div className="brand-orbit"><Sparkles size={26} /></div>
        <h2>{modelReady ? "Ready for local chat" : "Start a local conversation"}</h2>
        <p>{modelReady
          ? `${selectedModel?.displayName ?? "Your model"} is loaded and ready.`
          : selectedModel
            ? `${selectedModel.displayName} is selected but not loaded.`
            : "Select an installed GGUF model and a system prompt. Messages stay on this device."}</p>
        {selectedModel && !modelReady
          ? <button className="primary-button" disabled={modelOperation !== null} onClick={() => void loadModel(selectedModel)} type="button">{modelOperation === "load" ? <LoaderCircle className="spin" size={16} /> : <Sparkles size={16} />} Load model</button>
          : !selectedModel && <button className="secondary-button" onClick={() => setActiveView("models")} type="button"><Download size={16} /> Find a model</button>}
      </div> : <div aria-live="polite" className={`message-viewport ${error ? "has-error" : ""}`} onScroll={handleMessageScroll} ref={messageViewport}>
        {messages.map((message) => <article className={`chat-message ${message.role}`} key={message.id}>
          <div className="message-avatar">{message.role === "user" ? <User size={15} /> : <Bot size={15} />}</div>
          <div className="message-body">
            <header><strong>{message.role === "user" ? "You" : selectedModel?.displayName ?? "Assistant"}</strong>{message.usage && <small>{usageLabel(message.usage)}</small>}</header>
            {message.content
              ? <div className="message-content">{message.content}</div>
              : message.state === "complete"
                ? <div className="message-terminal error">No response text was returned</div>
                : message.state === "cancelled"
                  ? <div className="message-terminal">Generation stopped</div>
                  : message.state === "error"
                    ? <div className="message-terminal error">Generation failed</div>
                    : <div className="message-pending"><LoaderCircle className="spin" size={14} /> Thinking locally</div>}
          </div>
        </article>)}
      </div>}
      </div>

      <div className="composer">
        <textarea
          aria-label="Message"
          disabled={!modelReady || generating}
          maxLength={256 * 1024}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={handleComposerKeyDown}
          placeholder={composerPlaceholder}
          value={input}
        />
        <div className="composer-actions">
          <button className="icon-button" disabled title="Attach image" type="button"><ImagePlus size={18} /></button>
          {generating
            ? <button className="send-button stop" onClick={() => void cancelGeneration()} title="Stop generation" type="button"><Square size={15} /></button>
            : <button className="send-button" disabled={!modelReady || !input.trim()} onClick={() => void sendMessage()} title="Send message" type="button"><ArrowUp size={18} /></button>}
        </div>
      </div>
    </div>
  </div>;
}

function chatStatus(status: EngineRuntimeStatus | null, model: ModelRecord | null): string {
  if (!status) return "Reading runtime";
  if (!model) return status.lifecycle === "notInstalled" ? "Runtime unavailable" : "Select a model";
  if (status.modelId !== model.id || ["installed", "stopped"].includes(status.lifecycle)) return "Selected, not loaded";
  return {
    notInstalled: "Runtime unavailable",
    installed: "Selected, not loaded",
    starting: "Starting runtime",
    loadingModel: "Loading model",
    ready: "Ready on CPU",
    busy: "Generating locally",
    stopping: "Stopping runtime",
    stopped: "Selected, not loaded",
    crashed: "Runtime crashed",
    recovering: "Recovering runtime",
    error: "Runtime error",
  }[status.lifecycle];
}

function messageState(state: ChatGenerationState): MessageState {
  const states: Record<ChatGenerationState, MessageState> = {
    started: "streaming",
    completed: "complete",
    cancelled: "cancelled",
    failed: "error",
  };
  return states[state];
}

function usageLabel(usage: ChatUsage): string {
  const speed = usage.tokensPerSecond > 0 ? ` - ${usage.tokensPerSecond.toFixed(1)} tok/s` : "";
  return `${usage.outputTokens} tokens${speed}`;
}

function readLastModelId(): string | null {
  try {
    return window.localStorage.getItem(LAST_MODEL_KEY);
  } catch {
    return null;
  }
}

function writeLastModelId(modelId: string | null) {
  try {
    if (modelId) window.localStorage.setItem(LAST_MODEL_KEY, modelId);
    else window.localStorage.removeItem(LAST_MODEL_KEY);
  } catch {
    // A blocked renderer storage preference must not block local inference.
  }
}

function errorMessage(caught: unknown, fallback: string): string {
  if (typeof caught === "string") return caught;
  if (caught && typeof caught === "object" && "message" in caught && typeof caught.message === "string") return caught.message;
  return fallback;
}
