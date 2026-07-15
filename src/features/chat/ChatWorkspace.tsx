import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  Activity,
  AlertTriangle,
  ArrowUp,
  Bot,
  Check,
  Cpu,
  Download,
  FileText,
  Gauge,
  ImagePlus,
  LoaderCircle,
  PanelLeft,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Search,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  User,
  X,
} from "lucide-react";
import { bridge } from "../../services/bridge";
import { useAppStore } from "../../stores/app-store";
import type {
  ChatGenerationState,
  CompiledPrompt,
  ConversationSummary,
  EngineRuntimeStatus,
  ModelRecord,
  PromptSummary,
} from "../../types/domain";
import { calculateChatMetrics } from "./chat-metrics";
import {
  generationHistory,
  localMessagesFromConversation,
  rememberedConversation,
  type LocalMessage,
  type LocalMessageState,
} from "./conversation-history";
import {
  chatModelLabel,
  groupChatModels,
  isEngineActive,
  isSelectedModelReady,
} from "./model-selection";
import { chatMessagesWithSystemPrompt, rememberedPrompt } from "./prompt-selection";

const LAST_MODEL_KEY = "neuraloc.lastModelId";
const LAST_PROMPT_KEY = "neuraloc.lastPromptVersionId";
const LAST_CONVERSATION_KEY = "neuraloc.lastConversationId";
const AUTO_SCROLL_THRESHOLD_PX = 48;

interface PromptBinding extends CompiledPrompt {
  stableName: string;
  version: number;
}

export function ChatWorkspace() {
  const activeView = useAppStore((state) => state.activeView);
  const setActiveView = useAppStore((state) => state.setActiveView);
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [prompts, setPrompts] = useState<PromptSummary[]>([]);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationSearch, setConversationSearch] = useState("");
  const [runtimeStatus, setRuntimeStatus] = useState<EngineRuntimeStatus | null>(null);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [selectedPrompt, setSelectedPrompt] = useState<PromptBinding | null>(null);
  const [modelOperation, setModelOperation] = useState<"load" | "stop" | null>(null);
  const [conversationId, setConversationId] = useState<string>(() => crypto.randomUUID());
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [input, setInput] = useState("");
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [renamingConversationId, setRenamingConversationId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [exportingConversationId, setExportingConversationId] = useState<string | null>(null);
  const tokenSequences = useRef(new Map<string, number>());
  const stateSequences = useRef(new Map<string, number>());
  const usageSequences = useRef(new Map<string, number>());
  const messageViewport = useRef<HTMLDivElement | null>(null);
  const autoScrollToBottom = useRef(true);
  const previousView = useRef(activeView);

  const loadWorkspace = useCallback(async () => {
    setLoading(true);
    try {
      const [availableModels, status, availablePrompts, savedConversations] = await Promise.all([
        bridge.listModels(),
        bridge.getEngineStatus(),
        bridge.listPrompts(),
        bridge.listConversations(),
      ]);
      setModels(availableModels);
      setPrompts(availablePrompts);
      setConversations(savedConversations);
      setHistoryLoading(false);
      setRuntimeStatus(status);
      const savedConversation = rememberedConversation(savedConversations, readLastConversationId());
      if (savedConversation) {
        const detail = await bridge.getConversation(savedConversation.id);
        setConversationId(detail.conversation.id);
        setMessages(localMessagesFromConversation(detail.messages));
        setSelectedModelId(detail.conversation.modelId);
        writeLastModelId(detail.conversation.modelId);
        const restoredPrompt = detail.conversation.promptVersionId
          ? await compilePromptVersion(
              detail.conversation.promptVersionId,
              detail.conversation.promptName ?? "Saved prompt",
              detail.conversation.promptVersion ?? 1,
            )
          : null;
        setSelectedPrompt(restoredPrompt);
        writeLastPromptId(restoredPrompt?.versionId ?? null);
        return;
      }
      writeLastConversationId(null);
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
      const prompt = rememberedPrompt(availablePrompts, readLastPromptId());
      if (prompt) setSelectedPrompt(await compilePromptBinding(prompt));
    } catch (caught) {
      setError(errorMessage(caught, "Chat could not load local models and prompts."));
    } finally {
      setLoading(false);
    }
  }, []);

  const refreshConversations = useCallback(async (query: string) => {
    setHistoryLoading(true);
    try {
      setConversations(await bridge.listConversations(query));
    } catch (caught) {
      setError(errorMessage(caught, "Conversation history could not be refreshed."));
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadWorkspace();
  }, [loadWorkspace]);

  useEffect(() => {
    const timer = window.setTimeout(() => void refreshConversations(conversationSearch), 180);
    return () => window.clearTimeout(timer);
  }, [conversationSearch, refreshConversations]);

  useEffect(() => {
    const previous = previousView.current;
    previousView.current = activeView;
    if (activeView !== "chat" || previous !== "prompts") return;
    void bridge.listPrompts()
      .then(setPrompts)
      .catch((caught) => setError(errorMessage(caught, "The prompt library could not be refreshed.")));
  }, [activeView]);

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
  const activeConversation = conversations.find((conversation) => conversation.id === conversationId) ?? null;
  const modelReady = isSelectedModelReady(selectedModelId, runtimeStatus);
  const generating = activeJobId !== null;
  const runtimeAvailable = runtimeStatus?.lifecycle !== "notInstalled";
  const runtimeActive = isEngineActive(runtimeStatus);
  const chatMetrics = useMemo(
    () => calculateChatMetrics(
      messages,
      runtimeActive ? runtimeStatus?.contextSize ?? null : null,
      selectedPrompt?.estimatedTokens ?? 0,
    ),
    [messages, runtimeActive, runtimeStatus?.contextSize, selectedPrompt?.estimatedTokens],
  );

  async function openConversation(summary: ConversationSummary) {
    if (activeJobId) return;
    if (summary.id === conversationId) {
      setHistoryOpen(false);
      return;
    }
    setHistoryLoading(true);
    setError(null);
    try {
      const detail = await bridge.getConversation(summary.id);
      const restoredPrompt = detail.conversation.promptVersionId
        ? await compilePromptVersion(
            detail.conversation.promptVersionId,
            detail.conversation.promptName ?? "Saved prompt",
            detail.conversation.promptVersion ?? 1,
          )
        : null;
      setConversationId(detail.conversation.id);
      setMessages(localMessagesFromConversation(detail.messages));
      setSelectedModelId(detail.conversation.modelId);
      setSelectedPrompt(restoredPrompt);
      setInput("");
      setRenamingConversationId(null);
      autoScrollToBottom.current = true;
      writeLastConversationId(detail.conversation.id);
      writeLastModelId(detail.conversation.modelId);
      writeLastPromptId(restoredPrompt?.versionId ?? null);
      setHistoryOpen(false);
    } catch (caught) {
      setError(errorMessage(caught, `${summary.title} could not be opened.`));
    } finally {
      setHistoryLoading(false);
    }
  }

  function beginRenameConversation(summary: ConversationSummary) {
    setRenamingConversationId(summary.id);
    setRenameValue(summary.title);
  }

  async function saveConversationTitle(summary: ConversationSummary) {
    const title = renameValue.trim();
    if (!title || title === summary.title) {
      setRenamingConversationId(null);
      return;
    }
    setError(null);
    try {
      await bridge.renameConversation(summary.id, title);
      setRenamingConversationId(null);
      await refreshConversations(conversationSearch);
    } catch (caught) {
      setError(errorMessage(caught, "The conversation title could not be updated."));
    }
  }

  async function toggleConversationPinned(summary: ConversationSummary) {
    setError(null);
    try {
      await bridge.setConversationPinned(summary.id, !summary.pinned);
      await refreshConversations(conversationSearch);
    } catch (caught) {
      setError(errorMessage(caught, "The conversation pin could not be updated."));
    }
  }

  async function deleteConversation(summary: ConversationSummary) {
    if (!(await bridge.confirmDeleteConversation(summary.title))) return;
    setError(null);
    try {
      await bridge.deleteConversation(summary.id);
      if (summary.id === conversationId) {
        setConversationId(crypto.randomUUID());
        setMessages([]);
        setInput("");
        writeLastConversationId(null);
      }
      setRenamingConversationId(null);
      await refreshConversations(conversationSearch);
    } catch (caught) {
      setError(errorMessage(caught, "The conversation could not be deleted."));
    }
  }

  async function exportConversation(summary: ConversationSummary) {
    setExportingConversationId(summary.id);
    setError(null);
    try {
      const exported = await bridge.exportConversation(summary.id);
      downloadText(exported.fileName, exported.content, exported.mediaType);
    } catch (caught) {
      setError(errorMessage(caught, "The conversation could not be exported."));
    } finally {
      setExportingConversationId(null);
    }
  }

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
    if (messages.length > 0 && selectedModelId !== model.id) {
      const confirmed = await bridge.confirmModelConversationChange(model.displayName);
      if (!confirmed) return;
      await newConversation();
    }
    setSelectedModelId(model.id);
    writeLastModelId(model.id);
    await loadModel(model);
  }

  async function selectPrompt(value: string) {
    if (value === "manage") {
      setActiveView("prompts");
      return;
    }
    const summary = value === "none"
      ? null
      : prompts.find((prompt) => prompt.latestVersionId === value) ?? null;
    if (value !== "none" && !summary) return;
    if (selectedPrompt?.versionId === value || (!selectedPrompt && value === "none")) return;
    setError(null);
    try {
      const binding = summary ? await compilePromptBinding(summary) : null;
      if (messages.length > 0) {
        const confirmed = await bridge.confirmPromptConversationChange(binding?.stableName ?? "no custom prompt");
        if (!confirmed) return;
        await newConversation();
      }
      setSelectedPrompt(binding);
      writeLastPromptId(binding?.versionId ?? null);
    } catch (caught) {
      setError(errorMessage(caught, "The selected prompt version could not be compiled."));
    }
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
      terminalReason: null,
    };
    const assistantMessage: LocalMessage = {
      id: assistantMessageId,
      role: "assistant",
      content: "",
      state: "pending",
      usage: null,
      terminalReason: null,
    };
    const history = generationHistory(messages);
    const requestMessages = chatMessagesWithSystemPrompt(selectedPrompt?.content ?? null, history, content);
    autoScrollToBottom.current = true;
    setMessages((current) => [...current, userMessage, assistantMessage]);
    setInput("");
    setActiveJobId(jobId);
    writeLastConversationId(conversationId);
    tokenSequences.current.delete(jobId);
    stateSequences.current.delete(jobId);
    usageSequences.current.delete(jobId);
    try {
      const result = await bridge.startChatGeneration({
        jobId,
        conversationId,
        userMessageId: userMessage.id,
        messageId: assistantMessageId,
        sessionId: runtimeStatus.sessionId,
        promptVersionId: selectedPrompt?.versionId ?? null,
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
      void refreshConversations(conversationSearch);
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
    setRenamingConversationId(null);
    writeLastConversationId(null);
    setHistoryOpen(false);
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
    <aside className={`conversation-rail ${historyOpen ? "open" : ""}`}>
      <button className="new-chat-button" onClick={() => void newConversation()} type="button"><Plus size={17} /> New conversation</button>
      <div className="rail-search"><Search size={15} /><input aria-label="Search conversations" onChange={(event) => setConversationSearch(event.target.value)} placeholder="Search conversations" value={conversationSearch} /></div>
      <div className="conversation-list">
        {messages.length > 0 && !activeConversation && !conversationSearch && <div className="conversation-entry active transient">
          <button className="conversation-item active" type="button"><span>Current conversation</span><small>{selectedModel?.displayName ?? "Local chat"}{selectedPrompt ? ` / ${selectedPrompt.stableName} v${selectedPrompt.version}` : ""}</small></button>
        </div>}
        {conversations.map((summary) => <div className={`conversation-entry ${summary.id === conversationId ? "active" : ""}`} key={summary.id}>
          {renamingConversationId === summary.id
            ? <div className="conversation-rename">
                <input
                  aria-label="Conversation title"
                  autoFocus
                  maxLength={120}
                  onChange={(event) => setRenameValue(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void saveConversationTitle(summary);
                    if (event.key === "Escape") setRenamingConversationId(null);
                  }}
                  value={renameValue}
                />
                <button aria-label="Save title" className="conversation-action" disabled={!renameValue.trim()} onClick={() => void saveConversationTitle(summary)} title="Save title" type="button"><Check size={13} /></button>
                <button aria-label="Cancel rename" className="conversation-action" onClick={() => setRenamingConversationId(null)} title="Cancel rename" type="button"><X size={13} /></button>
              </div>
            : <>
                <button className={`conversation-item ${summary.id === conversationId ? "active" : ""}`} disabled={generating} onClick={() => void openConversation(summary)} type="button">
                  <span>{summary.title}</span>
                  <small>{summary.modelName} / {summary.messageCount.toLocaleString()} messages</small>
                </button>
                <div className="conversation-actions">
                  <button aria-label={summary.pinned ? "Unpin conversation" : "Pin conversation"} className="conversation-action" disabled={generating && summary.id === conversationId} onClick={() => void toggleConversationPinned(summary)} title={summary.pinned ? "Unpin conversation" : "Pin conversation"} type="button">{summary.pinned ? <PinOff size={13} /> : <Pin size={13} />}</button>
                  <button aria-label="Rename conversation" className="conversation-action" disabled={generating && summary.id === conversationId} onClick={() => beginRenameConversation(summary)} title="Rename conversation" type="button"><Pencil size={13} /></button>
                  <button aria-label="Export conversation" className="conversation-action" disabled={exportingConversationId !== null} onClick={() => void exportConversation(summary)} title="Export Markdown" type="button">{exportingConversationId === summary.id ? <LoaderCircle className="spin" size={13} /> : <Download size={13} />}</button>
                  <button aria-label="Delete conversation" className="conversation-action danger" disabled={generating && summary.id === conversationId} onClick={() => void deleteConversation(summary)} title="Delete conversation" type="button"><Trash2 size={13} /></button>
                </div>
              </>}
        </div>)}
        {historyLoading && conversations.length === 0
          ? <div className="conversation-empty"><LoaderCircle className="spin" size={15} /> Loading history</div>
          : !historyLoading && conversations.length === 0 && messages.length === 0
            ? <div className="conversation-empty">{conversationSearch ? "No matching conversations" : "No conversations yet"}</div>
            : null}
      </div>
    </aside>
    {historyOpen && <button aria-label="Close conversation history" className="conversation-backdrop" onClick={() => setHistoryOpen(false)} type="button" />}
    <div className="chat-workspace">
      <div className="chat-controls">
        <button aria-label="Conversation history" className="icon-button history-toggle" onClick={() => setHistoryOpen((current) => !current)} title="Conversation history" type="button"><PanelLeft size={17} /></button>
        <label>Model<select
          aria-label="Chat model"
          disabled={loading || modelOperation !== null || generating}
          onChange={(event) => void selectModel(event.target.value)}
          value={selectedModelId ?? "none"}
        >
          <option value="none">No model selected</option>
          {selectedModelId && !models.some((model) => model.id === selectedModelId)
            && <option disabled value={selectedModelId}>{activeConversation?.modelName ?? "Saved model"} - unavailable</option>}
          {groups.ready.length > 0 && <optgroup label={runtimeAvailable ? "Ready" : "Missing backend"}>
            {groups.ready.map((model) => <option disabled={!runtimeAvailable} key={model.id} value={model.id}>{chatModelLabel(model)}</option>)}
          </optgroup>}
          {groups.unavailable.length > 0 && <optgroup label="Unavailable">
            {groups.unavailable.map((model) => <option disabled key={model.id} value={model.id}>{model.displayName} - {model.verificationState}</option>)}
          </optgroup>}
          <option value="manage">Manage models...</option>
        </select></label>
        <label>System prompt<select
          aria-label="System prompt"
          disabled={loading || generating}
          onChange={(event) => void selectPrompt(event.target.value)}
          value={selectedPrompt?.versionId ?? "none"}
        >
          <option value="none">No custom prompt</option>
          {selectedPrompt && !prompts.some((prompt) => prompt.latestVersionId === selectedPrompt.versionId)
            && <option value={selectedPrompt.versionId}>{selectedPrompt.stableName} / v{selectedPrompt.version} (bound)</option>}
          {prompts.map((prompt) => <option key={prompt.latestVersionId} value={prompt.latestVersionId}>{prompt.pinned ? "Pinned - " : ""}{prompt.stableName} / v{prompt.latestVersion}</option>)}
          <option value="manage">Manage prompt library...</option>
        </select></label>
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
            <header><strong>{message.role === "user" ? "You" : selectedModel?.displayName ?? activeConversation?.modelName ?? "Assistant"}</strong></header>
            {message.content
              ? <div className="message-content">{message.content}</div>
              : message.state === "complete"
                ? <div className="message-terminal error">No response text was returned</div>
                : message.state === "cancelled"
                  ? <div className="message-terminal">Generation stopped</div>
                  : message.state === "interrupted"
                    ? <div className="message-terminal error">Generation was interrupted before the app closed</div>
                  : message.state === "error"
                  ? <div className="message-terminal error">Generation failed</div>
                  : <div className="message-pending"><LoaderCircle className="spin" size={14} /> Thinking locally</div>}
            {message.usage && <footer className="message-usage">
              <span>{message.usage.outputTokens.toLocaleString()} output</span>
              <span>{message.usage.promptTokens.toLocaleString()} prompt</span>
              <span>{message.usage.tokensPerSecond > 0 ? `${message.usage.tokensPerSecond.toFixed(1)} tok/s` : "Speed unavailable"}</span>
            </footer>}
          </div>
        </article>)}
      </div>}
      </div>

      <div aria-label="Live conversation metrics" className="chat-status-strip">
        <div className="context-status" title="Current conversation tokens and loaded context capacity">
          <Gauge size={13} />
          <span>Context</span>
          <strong>{tokenMetric(chatMetrics.contextTokens, chatMetrics.contextApproximate)} / {tokenMetric(chatMetrics.contextCapacity)}</strong>
          <progress
            aria-label="Context window usage"
            max={chatMetrics.contextCapacity ?? 1}
            value={Math.min(chatMetrics.contextTokens ?? 0, chatMetrics.contextCapacity ?? 1)}
          />
          <small>{chatMetrics.contextPercent === null ? "--" : `${chatMetrics.contextPercent}%`}</small>
        </div>
        <div title="Current generation state"><Activity size={13} /><span>{generating ? "Generating" : modelOperation === "load" ? "Loading" : modelReady ? "Ready" : "Idle"}</span></div>
        <div title="Tokens in the latest response"><span>Output</span><strong>{tokenMetric(chatMetrics.outputTokens, chatMetrics.outputApproximate)}</strong></div>
        <div className="prompt-route" title="Immutable system prompt bound to this conversation"><FileText size={13} /><span>Prompt</span><strong>{selectedPrompt ? `${selectedPrompt.stableName} v${selectedPrompt.version}` : "None"}</strong></div>
        <div title="Latest measured generation speed"><span>Speed</span><strong>{chatMetrics.tokensPerSecond && chatMetrics.tokensPerSecond > 0 ? `${chatMetrics.tokensPerSecond.toFixed(1)} tok/s` : "--"}</strong></div>
        <div className="runtime-route" title="Active inference route and backend build"><Cpu size={13} /><span>{runtimeActive ? `CPU / ${runtimeStatus?.backendVersion ?? "llama.cpp"}` : "CPU / offline"}</span></div>
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

function messageState(state: ChatGenerationState): LocalMessageState {
  const states: Record<ChatGenerationState, LocalMessageState> = {
    started: "streaming",
    completed: "complete",
    cancelled: "cancelled",
    failed: "error",
  };
  return states[state];
}

function tokenMetric(value: number | null, approximate = false): string {
  if (value === null) return "--";
  return `${approximate ? "~" : ""}${value.toLocaleString()}`;
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

async function compilePromptBinding(prompt: PromptSummary): Promise<PromptBinding> {
  return compilePromptVersion(prompt.latestVersionId, prompt.stableName, prompt.latestVersion);
}

async function compilePromptVersion(versionId: string, stableName: string, version: number): Promise<PromptBinding> {
  const compiled = await bridge.compilePrompt(versionId);
  if (new Blob([compiled.content]).size > 256 * 1024) {
    throw new Error("This prompt exceeds the current 256 KiB chat system-message limit.");
  }
  return {
    ...compiled,
    stableName,
    version,
  };
}

function readLastConversationId(): string | null {
  try {
    return window.localStorage.getItem(LAST_CONVERSATION_KEY);
  } catch {
    return null;
  }
}

function writeLastConversationId(conversationId: string | null) {
  try {
    if (conversationId) window.localStorage.setItem(LAST_CONVERSATION_KEY, conversationId);
    else window.localStorage.removeItem(LAST_CONVERSATION_KEY);
  } catch {
    // A blocked renderer storage preference must not affect durable conversation storage.
  }
}

function readLastPromptId(): string | null {
  try {
    return window.localStorage.getItem(LAST_PROMPT_KEY);
  } catch {
    return null;
  }
}

function writeLastPromptId(versionId: string | null) {
  try {
    if (versionId) window.localStorage.setItem(LAST_PROMPT_KEY, versionId);
    else window.localStorage.removeItem(LAST_PROMPT_KEY);
  } catch {
    // A blocked renderer storage preference must not change conversation behavior.
  }
}

function downloadText(fileName: string, content: string, mediaType: string) {
  const url = URL.createObjectURL(new Blob([content], { type: mediaType }));
  const link = window.document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function errorMessage(caught: unknown, fallback: string): string {
  if (typeof caught === "string") return caught;
  if (caught && typeof caught === "object" && "message" in caught && typeof caught.message === "string") return caught.message;
  return fallback;
}
