-- 单机版（solo v5，纯本机）：大厅里点"开始"时（WBP_MatchMainButton.lua 调 M.Open）直接本地开图，不匹配、不连对局服务器。
--
-- 开图：UBattleService:StartLocalBattle(所选关卡的地图)——游戏自己的本地战斗，开图时向本机大厅拉一次大厅角色（装备、技能、外观），
--   所以进图的就是大厅里的角色和装备。大厅是唯一的存档：这里只把这一局的结局记进本机文件，回大厅时由本机大厅写回角色。
-- 结局文件（%LOCALAPPDATA%\Dungeonborne\Saved\SoloRaid.txt，先写 .tmp 再换名，写一半崩了也不坏）：
--   第 1 行：v3 <局号> <状态> <序号> <开局 Unix 秒> <关卡 id>
--   第 2 行：条目;条目...（可为空）
--     穿戴：w,<穿戴槽 tgf.EnItemWearingPart>,<数量>,<tgf.DItem 十六进制>
--     背包：b,<背包类型 0 主背包/1 附加背包>,<行>,<列>,<数量>,<tgf.DItem 十六进制>
--   状态：0 对局中；1 死亡（大厅清空身上与背包、补发初始装备）；2 撤离；3 离开（按条目写回）。
--   金手指 keep_on_death=1 时死亡记成 3（按死亡时身上 + 背包保留）。游戏崩了 / 被关掉：下次启动时（Start）把 0 改成 3。
-- 撤离：进传送门成功时 PlayerState:GetIsEscapeQuit() 变 true，同一刻原生会清空背包与装备，所以撤离记的是之前最后一份。
-- 死亡：角色持续"死亡或濒死"超过 DeadConfirm 秒才算。
-- 爆率倍率（金手指 loot_mult=N > 1）：箱子刚被打开、怪物刚死时，按它自己的掉落源再掷 N-1 次 UBattleUtility.Loot，
--   箱子的放进箱子里；怪物身上有背包组件就放进它身上，没有就直接放进玩家背包。
-- 地块加载（2026-10-21，用户："怪物和角色有时候会卡在地下一半"）：城堡 / 罪恶尽头是 World Partition 大地图，联机时对局服务器整张图
--   全加载，单机本地开图是客户端自己算物理、只加载玩家附近的地块——怪物或角色在地面还没加载的地方就掉进去卡住。进图后执行
--   wp.Runtime.OverrideRuntimeSpatialHashLoadingRange（各网格加载范围放大到 wp_range 厘米）和 wp.Runtime.BlockOnSlowStreaming 1
--   （地块跟不上时引擎先等它加载完）。范围越大越不容易陷地、越吃内存和帧数；wp_range=0 关掉。
-- 配置 SoloConfig.txt（单机启动器的"金手指"写它；每次进图重读）：keep_on_death=0|1、loot_mult=1~10、wp_range=厘米（默认 100000）。
-- 新手关、训练场、无瑕试炼不记。日志：Saved\Logs\SoloBoot.log。
local M = {}

local Saved = (os.getenv("LOCALAPPDATA") or ".") .. "/Dungeonborne/Saved/"
local RaidPath = Saved .. "SoloRaid.txt"
local CfgPath = Saved .. "SoloConfig.txt"
local LogPath = Saved .. "Logs/SoloBoot.log"
local ChestPath = "/Game/TGF/Environment/Chests/BP_InteractableChestBase.BP_InteractableChestBase_C"

local Alive, Dead, Escaped, Left = 0, 1, 2, 3
local Interval = 0.5
local DeadConfirm = 2.0
local RescanEvery = 10
local Grids = 4 -- OverrideRuntimeSpatialHashLoadingRange 按网格设，逐个设前几个
-- tgf.EnItemWearingPart（局内装备槽同号）；双手武器同时占 3、4 两格，只记先出现的那件。
local WearSlots = {1, 2, 3, 4, 11, 12, 13, 14, 15, 16, 17, 18, 41, 42, 43, 51,
  101, 102, 103, 104, 105, 106, 107, 108, 109}

local Hex = {}
for i = 0, 255 do
  Hex[i] = string.format("%02x", i)
end

local SessionId = string.format("%x%04x", os.time(), math.random(0, 0xffff))
local RaidNo = 0
local Seq = 0
local PendingLevel = 0 -- 开始按钮记下的关卡
local Cfg = {}
local Clock = 0
local Raid = nil -- {Id, Start, Level, World, Since, Last, Dead, DeadSince, Escaped, Boost}
local LastError = nil

local function Log(Text)
  pcall(function()
    local F = io.open(LogPath, "a")
    if F then
      F:write(os.date("%Y-%m-%d %H:%M:%S "), tostring(Text), "\n")
      F:close()
    end
  end)
end

local function ReadFile(Path)
  local F = io.open(Path, "rb")
  if not F then
    return nil
  end
  local Text = F:read("a")
  F:close()
  return Text
end

local function WriteFile(Path, Text)
  local Tmp = Path .. ".tmp"
  local F = assert(io.open(Tmp, "wb"))
  F:write(Text)
  F:close()
  os.remove(Path)
  assert(os.rename(Tmp, Path))
end

local function LoadConfig()
  local C = {keep_on_death = 0, loot_mult = 1, wp_range = 100000}
  local Text = ReadFile(CfgPath)
  if Text then
    for Line in Text:gmatch("[^\r\n]+") do
      local K, V = Line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
      if K and V ~= "" then
        C[K] = tonumber(V) or V
      end
    end
  end
  return C
end

-- WriteRaid 把这一局的状态整份写进结局文件。
local function WriteRaid(Status, Entries)
  Seq = Seq + 1
  WriteFile(RaidPath, string.format("v3 %s %d %d %d %d\n%s\n", Raid.Id, Status, Seq, Raid.Start, Raid.Level, Entries or ""))
end

-- Recover：上次游戏退出时那一局还是"对局中"（崩了 / 被关掉）→ 改成离开，按最后的身上 + 背包写回。
local function Recover()
  if not ReadFile(RaidPath) and ReadFile(RaidPath .. ".tmp") then
    os.rename(RaidPath .. ".tmp", RaidPath) -- 换名换到一半
  end
  local Text = ReadFile(RaidPath)
  if not Text then
    return
  end
  local Head, Rest = Text:match("^([^\n]*)\n?(.*)$")
  local Id, Status, Old, Start, Level = Head:match("^v3 (%S+) (%d+) (%d+) (%d+) (%d+)%s*$")
  if Id and tonumber(Status) == Alive then
    WriteFile(RaidPath, string.format("v3 %s %d %d %s %s\n%s", Id, Left, tonumber(Old) + 1, Start, Level, Rest))
    Log("raid " .. Id .. " was still in progress when the game closed: marked as left")
  end
end
M._Recover = Recover -- 单测用

local function Subsystem(Class)
  return UE.UTGFBaseFunctionLibrary.GetGameInstanceSubsystem(Class)
end

-- BattleWorld：当前是对局地图（城堡 / 罪恶尽头）则返回 World 与地图名。
local function BattleWorld()
  local Ok, World = pcall(function()
    return Subsystem(UE.UBattleService):GetWorld()
  end)
  if not Ok or not World then
    return nil
  end
  local Name = World:GetName()
  if Name:find("GothicCastle") or Name:find("Dungeon_Official") then
    return World, Name
  end
  return nil, Name
end

-- M.Open：开始按钮点下时直接本地开图。
function M.Open(MapPath, LevelId)
  PendingLevel = tonumber(LevelId) or 0
  local Ok, Err = pcall(function()
    Subsystem(UE.UBattleService):StartLocalBattle(MapPath)
  end)
  Log("StartLocalBattle(" .. MapPath .. ") level " .. tostring(LevelId) .. " " .. (Ok and "called" or ("failed: " .. tostring(Err))))
  return Ok
end

local function ItemHex(Item)
  local Buf = Item.DItemBuffer.Buffer
  local T = {}
  for i = 1, Buf:Length() do
    T[i] = Hex[Buf:Get(i)]
  end
  return table.concat(T)
end

local function Count(Item)
  local Ok, N = pcall(function()
    return Item:GetStackCount()
  end)
  return Ok and N or 0
end

-- Snapshot：身上 + 背包的条目（格式见文件头）。
local function Snapshot(Pawn)
  local Parts = {}
  local EM = Pawn:GetComponentByClass(UE.UTgfEquipmentManagerComponent)
  local IM = Pawn:GetComponentByClass(UE.UTgfInventoryManagerComponent)
  if EM then
    local Seen = {}
    for _, Slot in ipairs(WearSlots) do
      local Equip = EM:GetEquipBySlot(Slot, -1)
      local Item = Equip and Equip.InventoryInstance
      if Item and not Seen[Item:GetName()] then
        Seen[Item:GetName()] = true
        Parts[#Parts + 1] = string.format("w,%d,%d,%s", Slot, Count(Item), ItemHex(Item))
      end
    end
  end
  if IM then
    local Items = IM:GetAllItems()
    for i = 1, Items:Length() do
      local Item = Items:Get(i)
      local Ok, InvType, Point = IM:GetPositionByItemInstance(Item)
      if Ok and Point then
        Parts[#Parts + 1] = string.format("b,%d,%d,%d,%d,%s", InvType or 0, Point.X, Point.Y, Count(Item), ItemHex(Item))
      end
    end
  end
  return table.concat(Parts, ";"), #Parts
end

local function IsDead(Pawn)
  local Ok, D = pcall(function()
    return Pawn:IsDeadOrDying()
  end)
  return Ok and D == true
end

local function IsEscaped(PC)
  local Ok, E = pcall(function()
    return PC.PlayerState:GetIsEscapeQuit()
  end)
  return Ok and E == true
end

-- WideStreaming：放大地块加载范围、加载跟不上时先等（见文件头）。
local function WideStreaming(World, PC)
  local Range = math.floor(tonumber(Cfg.wp_range) or 0)
  if Range <= 0 then
    return
  end
  for G = 0, Grids - 1 do
    UE.UKismetSystemLibrary.ExecuteConsoleCommand(World,
      string.format("wp.Runtime.OverrideRuntimeSpatialHashLoadingRange -grid=%d -range=%d", G, Range), PC)
  end
  UE.UKismetSystemLibrary.ExecuteConsoleCommand(World, "wp.Runtime.BlockOnSlowStreaming 1", PC)
  Log(string.format("world streaming: loading range %d cm on %d grids, block on slow streaming", Range, Grids))
end

-- Extra：按 Actor 自己的掉落源再掷 Times 次，掉出来的放进 Into（背包组件）。
local function Extra(Actor, Times, PC, Into)
  local Src = Actor:GetClass():GetName()
  local PS = PC.PlayerState
  local Pss = UE.TArray(UE.APlayerState)
  Pss:Add(PS)
  local N = 0
  for _ = 1, Times do
    local R = UE.UBattleUtility.Loot(Actor, Src, "", Pss, PS, Src, 0)
    for i = 1, R:Length() do
      local It = R:Get(i)
      local Ok, C = pcall(function()
        return It:GetStackCount()
      end)
      UE.UBattleUtility.AddItem(Into, It:GetTypeId(), (Ok and C and C > 0) and C or 1, "")
      N = N + 1
    end
  end
  return N
end

-- BoostLoot：爆率倍率。箱子按 bOpened 由假变真认"刚打开"；怪物（不是自己的角色）按死亡认，每个只加一次。
local function BoostLoot(World, PC, Pawn)
  local Mult = math.floor(tonumber(Cfg.loot_mult) or 1)
  if Mult <= 1 then
    return
  end
  local B = Raid.Boost
  if not B then
    B = {Chests = {}, Mobs = {}, Opened = {}, Done = {}, LastScan = -RescanEvery}
    Raid.Boost = B
  end
  if Clock - B.LastScan >= RescanEvery then
    B.LastScan = Clock
    B.Chests, B.Mobs = {}, {}
    local Arr = UE.UGameplayStatics.GetAllActorsOfClass(World, UE.UClass.Load(ChestPath))
    for i = 1, Arr:Length() do
      local C = Arr:Get(i)
      local N = C:GetName()
      B.Chests[#B.Chests + 1] = C
      if B.Opened[N] == nil then
        B.Opened[N] = C.bOpened == true -- 扫到时已经开着的不算
      end
    end
    Arr = UE.UGameplayStatics.GetAllActorsOfClass(World, UE.ALyraCharacter)
    for i = 1, Arr:Length() do
      local Mob = Arr:Get(i)
      if Mob ~= Pawn then
        B.Mobs[#B.Mobs + 1] = Mob
      end
    end
  end
  for _, C in ipairs(B.Chests) do
    pcall(function()
      local N = C:GetName()
      if B.Opened[N] == false and C.bOpened == true then
        B.Opened[N] = true
        local IM = C:GetComponentByClass(UE.UTgfInventoryManagerComponent)
        if IM then
          Log(string.format("loot x%d: chest %s +%d", Mult, C:GetClass():GetName(), Extra(C, Mult - 1, PC, IM)))
        end
      end
    end)
  end
  for _, Mob in ipairs(B.Mobs) do
    pcall(function()
      local N = Mob:GetName()
      if not B.Done[N] and IsDead(Mob) then
        B.Done[N] = true
        local IM = Mob:GetComponentByClass(UE.UTgfInventoryManagerComponent)
          or Pawn:GetComponentByClass(UE.UTgfInventoryManagerComponent)
        Log(string.format("loot x%d: %s +%d", Mult, Mob:GetClass():GetName(), Extra(Mob, Mult - 1, PC, IM)))
      end
    end)
  end
end

local function Tick()
  Clock = Clock + Interval
  local World, Name = BattleWorld()
  if not World then
    if Raid then
      if not Raid.Escaped and not Raid.Dead then
        WriteRaid(Left, Raid.Last)
        Log("left the map " .. Raid.World .. " without escaping or dying: kept the last items")
      end
      Raid = nil
    end
    return
  end
  if not Raid then
    Cfg = LoadConfig() -- 金手指在启动器里改了，下一局就按新的
    RaidNo = RaidNo + 1
    Raid = {Id = SessionId .. "-" .. RaidNo, Start = os.time(), Level = PendingLevel, World = Name, Since = Clock, Last = ""}
    PendingLevel = 0
    WriteRaid(Alive, "")
    Log(string.format("entered %s (raid %s; keep_on_death %s, loot_mult %s)", Name, Raid.Id, tostring(Cfg.keep_on_death),
      tostring(Cfg.loot_mult)))
  end
  if Raid.Dead or Raid.Escaped then
    return
  end
  local PC = UE.UGameplayStatics.GetPlayerController(World, 0)
  if PC and not Raid.Streamed then
    Raid.Streamed = true
    local Ok, Err = pcall(WideStreaming, World, PC)
    if not Ok then
      Log("world streaming failed: " .. tostring(Err))
    end
  end
  if PC and IsEscaped(PC) then
    Raid.Escaped = true
    WriteRaid(Escaped, Raid.Last)
    Log("escaped: saved the last items")
    return
  end
  local Pawn = PC and PC:K2_GetPawn()
  if not Pawn then
    return
  end
  if IsDead(Pawn) then
    Raid.DeadSince = Raid.DeadSince or Clock
    if Clock - Raid.DeadSince >= DeadConfirm then
      Raid.Dead = true
      if tonumber(Cfg.keep_on_death) == 1 then
        WriteRaid(Left, Raid.Last)
        Log("dead, keep_on_death: nothing lost")
      else
        WriteRaid(Dead, Raid.Last)
        Log("dead: everything carried is lost")
      end
    end
    return
  end
  Raid.DeadSince = nil
  local Ok, Err = pcall(BoostLoot, World, PC, Pawn)
  if not Ok and Err ~= Raid.BoostErr then
    Raid.BoostErr = Err
    Log("loot boost error: " .. tostring(Err))
  end
  local Body, N = Snapshot(Pawn)
  if N == 0 or Body == Raid.Last then
    return
  end
  Raid.Last = Body
  WriteRaid(Alive, Body)
end

function M.Start()
  if rawget(_G, "__SoloBootStarted") then
    return true
  end
  if not (UE and UE.UTGFBaseFunctionLibrary) then
    return false
  end
  local Timers, Inst = Subsystem(UE.UTimerSubsystem), Subsystem(UE.UBattleService)
  if not Timers or not Inst then
    return false
  end
  rawset(_G, "__SoloBootStarted", true)
  Cfg = LoadConfig()
  local Ok, Err = pcall(Recover)
  if not Ok then
    Log("recover failed: " .. tostring(Err))
  end
  Timers:AddTimer(Inst, function()
    local Ok2, Err2 = pcall(Tick)
    if not Ok2 and tostring(Err2) ~= LastError then
      LastError = tostring(Err2) -- 同一个错误只记一次
      Log("tick error: " .. LastError)
    end
  end, Interval, true)
  Log("SoloBoot started (session " .. SessionId .. ")")
  return true
end

return M
