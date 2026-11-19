-- DungeonborneToast — server toasts: the game's system notice at the top of the screen with any text from the server
-- (owner's item 2, 18.11; backend core/backend/src/rpc/handlers/toast.rs, admin POST /admin/v1/toast).
--
-- The server sends a system chat message (DChatMessagemSysNotify, the player's system channel) whose text starts with the
-- invisible marker U+200B. The client creates TGFChatReceiveMessageEvent for every received chat message; here we read
-- its text (UCultureBlueprintLibrary.GetCultureText — the same as WBP_Chat_MessageItem.SetSystemMessage) and, if it carries
-- the marker, show it with UToastWindow.ShowToast — the same notice the game uses for error codes. Without this mod the
-- player sees the text in the system chat.
--
-- Threads (docs/CRASH_LONG_PLAY.md, as in RuClient): the creation callback runs while the object is being built — nothing
-- is read there; the path is remembered and the work goes to the game thread (ExecuteInGameThread), where a live object
-- is looked up by the path. Every call is in pcall. Not checked in game yet (18.11): see the report of item 2.

local MARK = "\226\128\139"        -- U+200B in UTF-8
local MAX_AGE = 300                -- s: older messages (the chat snapshot at login) are not shown as a toast
local Shown = {}                   -- message sequence -> true: a message is shown once

local function Log(s) print("[Toast] " .. tostring(s) .. "\n") end

-- the game server process (-dbserver) has no screen
local IsServer = false
pcall(function()
    local cl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary"):GetCommandLine():ToString():lower()
    IsServer = (" " .. cl .. " "):find("%s%-dbserver%s") ~= nil
end)
if IsServer then
    Log("game server — off")
    return
end

local function PathOf(obj)
    local full = ""
    pcall(function() full = obj:GetFullName() end)
    return full:match("^%S+%s+(.+)$") or ""
end

local function Str(v)
    if type(v) == "string" then return v end
    local s = nil
    pcall(function() s = v:ToString() end)
    return s
end

local function Show(Text)
    local W = StaticFindObject("/Script/TGFGame.Default__ToastWindow")
    if not (W and W:IsValid()) then Log("no ToastWindow"); return end
    W:ShowToast(FText(Text))
end

local function Handle(Path)
    local Ev = nil
    pcall(function() Ev = StaticFindObject(Path) end)
    if not (Ev and Ev:IsValid()) then return end
    local Msg = Ev.ChatMessage
    if not Msg.SystemNotify.bIsValid then return end
    local Lib = StaticFindObject("/Script/TGFGame.Default__CultureBlueprintLibrary")
    local Text = Str(Lib:GetCultureText(Msg.SystemNotify.CultureTextMap))
    if not Text or Text:sub(1, #MARK) ~= MARK then return end
    local Seq = tostring(Msg.Sequence)
    if Shown[Seq] then return end
    Shown[Seq] = true
    local Sent = tonumber(Msg.SendTimeStamp) or 0
    if Sent > 0 and math.abs(os.time() - Sent) > MAX_AGE then
        Log("old message " .. Seq .. " — not shown")
        return
    end
    Text = Text:sub(#MARK + 1)
    Log("show: " .. Text)
    Show(Text)
end

NotifyOnNewObject("/Script/TGFGame.TGFChatReceiveMessageEvent", function(Ev)
    local Path = PathOf(Ev)
    if Path == "" then return end
    ExecuteInGameThread(function()
        local Ok, Err = pcall(Handle, Path)
        if not Ok then Log("error: " .. tostring(Err)) end
    end)
end)

Log("ready")
