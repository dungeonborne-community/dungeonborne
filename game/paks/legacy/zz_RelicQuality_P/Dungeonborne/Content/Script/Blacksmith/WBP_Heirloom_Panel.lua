-- Крафт реликвии младшего качества: стрелки выбора качества на главной панели кузнеца.
-- Оригинальный модуль игры лежит рядом как WBP_Heirloom_Panel_Orig.lua.
-- Отключить: убрать пак zz_RelicQuality_P (клиент) и relic_lower_quality в deadmoney.toml (сервер).
--
-- Показ: оригинальный UpdateHeirloomGroup берёт качество из group:GetDisplayHeirloomId(). На время вызова подменяем
-- группу в сервисе на прокси с выбранным id, и он сам показывает название, характеристики и цену этого качества.
-- Крафт: окно кузнеца присылает текущее качество в OnHeirloomInstantiateClicked, там меняем id на выбранный.
local M = require("Blacksmith.WBP_Heirloom_Panel_Orig")

local BUTTON_CLASS = "/Game/TGF/UI/Common/Buttons/WBP_Tgf_ButtonBase.WBP_Tgf_ButtonBase_C"
local STYLE_PREV = "/Game/TGF/UI/Common/Buttons/ButtonStyle_Heirloom_ButtonPrev.ButtonStyle_Heirloom_ButtonPrev_C"
local STYLE_NEXT = "/Game/TGF/UI/Common/Buttons/ButtonStyle_Heirloom_ButtonNext.ButtonStyle_Heirloom_ButtonNext_C"
local BUTTON_SIZE = 48
local BUTTON_GAP = 12
local PLACE_DELAY = 0.3
local LOG_PATH = (os.getenv("LOCALAPPDATA") or ".") .. "/Dungeonborne/Saved/Logs/RelicQuality.log"

local function Log(Msg)
  pcall(function()
    local F = io.open(LOG_PATH, "a")
    if F then
      F:write(os.date("%H:%M:%S ") .. tostring(Msg) .. "\n")
      F:close()
    end
  end)
end

local function Service()
  return UE.UTGFBaseFunctionLibrary.GetGameInstanceSubsystem(UE.UHeirloomService)
end

-- Качества группы от младшего к текущему: цепочка unlock_condition таблицы ExcelGearHeirloomGuide.
local function QualityChain(CurId)
  local Guide = TGFTableManager:GetTabbleByName("ExcelGearHeirloomGuide")
  local Chain = { CurId }
  local Id = CurId
  while Id do
    local Row = Guide:GetByIndex("id", Id)
    local Prev = nil
    if Row and Row.unlock_condition then
      for _, Value in pairs(Row.unlock_condition) do
        Prev = Value
        break
      end
    end
    if Prev then
      table.insert(Chain, 1, Prev)
    end
    Id = Prev
  end
  return Chain
end

-- Прокси группы: всё как у настоящей, кроме показываемого качества.
local function GroupProxy(Group, DisplayId)
  return setmetatable({}, {
    __index = function(_, Key)
      if Key == "GetDisplayHeirloomId" then
        return function()
          return DisplayId
        end
      end
      local Value = Group[Key]
      if type(Value) == "function" then
        return function(_, ...)
          return Value(Group, ...)
        end
      end
      return Value
    end,
  })
end

-- Внешний холст панели. Холсты в BP не объявлены переменными, поэтому идём вверх от кнопки «Изготовить».
local function FindOuterCanvas(self)
  local Canvas = nil
  local Widget = self.BtnInstantiate
  while Widget do
    if Widget.AddChildToCanvas then
      Canvas = Widget
    end
    Widget = Widget:GetParent()
  end
  return Canvas
end

local function MakeButton(self, StylePath, Handler)
  local Button = UE.UWidgetBlueprintLibrary.Create(self, UE.UClass.Load(BUTTON_CLASS))
  Button:SetStyle(UE.UClass.Load(StylePath))
  local Slot = self.RelicCanvas:AddChildToCanvas(Button)
  Slot:SetAutoSize(false)
  Slot:SetSize(UE.FVector2D(BUTTON_SIZE, BUTTON_SIZE))
  Slot:SetAlignment(UE.FVector2D(0, 0.5))
  self:AddWidgetDelegate(Button.OnButtonBaseClicked, Handler, self)
  Button:SetVisibility(UE.ESlateVisibility.Collapsed)
  return Button
end

-- Стрелки по бокам кнопки «Изготовить». Геометрия готова только после отрисовки, поэтому вызов по таймеру.
local function PlaceButtons(self)
  local Lib = UE.USlateBlueprintLibrary
  local Geometry = self.BtnInstantiate:GetCachedGeometry()
  local Parent = self.RelicCanvas:GetCachedGeometry()
  local Size = Lib.GetLocalSize(Geometry)
  if Size.X <= 0 then
    return false
  end
  local Left = Lib.AbsoluteToLocal(Parent, Lib.LocalToAbsolute(Geometry, UE.FVector2D(0, Size.Y / 2)))
  local Right = Lib.AbsoluteToLocal(Parent, Lib.LocalToAbsolute(Geometry, UE.FVector2D(Size.X, Size.Y / 2)))
  -- Справа от кнопки стоит кнопка справки BtnInstantiateInfo: стрелка идёт за ней.
  local Info = self.BtnInstantiateInfo:GetCachedGeometry()
  local InfoSize = Lib.GetLocalSize(Info)
  local InfoRight = Lib.AbsoluteToLocal(Parent, Lib.LocalToAbsolute(Info, UE.FVector2D(InfoSize.X, InfoSize.Y / 2)))
  local NextX = math.max(Right.X, InfoRight.X) + BUTTON_GAP
  self.RelicPrev.Slot:SetPosition(UE.FVector2D(Left.X - BUTTON_GAP - BUTTON_SIZE, Left.Y))
  self.RelicNext.Slot:SetPosition(UE.FVector2D(NextX, Right.Y))
  return true
end

local function UpdateButtons(self)
  local Visible = UE.ESlateVisibility.Visible
  local Collapsed = UE.ESlateVisibility.Collapsed
  local Chain = self.RelicChain or {}
  local Index = self.RelicIndex or 1
  self.RelicPrev:SetVisibility(Index > 1 and Visible or Collapsed)
  self.RelicNext:SetVisibility(Index < #Chain and Visible or Collapsed)
end

local OrigUpdate = M.UpdateHeirloomGroup
local OrigClicked = M.OnHeirloomInstantiateClicked

local function SelectedId(self)
  return self.RelicChain and self.RelicChain[self.RelicIndex or 0] or nil
end

-- Обновляет выбор под текущую группу: новая группа или новое старшее качество сбрасывают его на старшее.
local function SyncChain(self, Group)
  local CurId = Group:GetCurHeirloomId()
  if self.RelicGroupId ~= Group.GroupID or self.RelicTopId ~= CurId then
    self.RelicGroupId = Group.GroupID
    self.RelicTopId = CurId
    self.RelicChain = (CurId and CurId ~= 0 and Group:GetIsUnlock()) and QualityChain(CurId) or {}
    self.RelicIndex = #self.RelicChain
  end
end

local function SetupButtons(self)
  if self.RelicPrev then
    return
  end
  self.RelicCanvas = FindOuterCanvas(self)
  if not self.RelicCanvas then
    error("нет холста над BtnInstantiate")
  end
  self.RelicPrev = MakeButton(self, STYLE_PREV, self.OnRelicPrevClicked)
  self.RelicNext = MakeButton(self, STYLE_NEXT, self.OnRelicNextClicked)
end

function M:UpdateHeirloomGroup(...)
  local Svc = Service()
  local Group = Svc:GetCurHeirloomGroup()
  local Ok, Err = pcall(function()
    if Group then
      SyncChain(self, Group)
      SetupButtons(self)
      UpdateButtons(self)
      self:BindTimer(PLACE_DELAY, function()
        local Placed, PlaceErr = pcall(PlaceButtons, self)
        if not Placed then
          Log("PlaceButtons: " .. tostring(PlaceErr))
        end
      end)
    end
  end)
  if not Ok then
    Log("выбор качества: " .. tostring(Err))
    self.RelicChain = nil
  end

  local Id = SelectedId(self)
  if not Group or not Id or Id == self.RelicTopId then
    return OrigUpdate(self, ...)
  end
  -- Младшее качество: оригинал читает группу из сервиса, поэтому на время вызова там прокси.
  Svc.CurHeirloomGroup = GroupProxy(Group, Id)
  local Called, CallErr = pcall(OrigUpdate, self, ...)
  Svc.CurHeirloomGroup = Group
  if not Called then
    Log("показ качества " .. tostring(Id) .. ": " .. tostring(CallErr))
    self.RelicIndex = #self.RelicChain
    return OrigUpdate(self, ...)
  end
end

function M:OnRelicPrevClicked()
  if self.RelicIndex and self.RelicIndex > 1 then
    self.RelicIndex = self.RelicIndex - 1
    self:UpdateHeirloomGroup()
  end
end

function M:OnRelicNextClicked()
  if self.RelicIndex and self.RelicChain and self.RelicIndex < #self.RelicChain then
    self.RelicIndex = self.RelicIndex + 1
    self:UpdateHeirloomGroup()
  end
end

function M:OnHeirloomInstantiateClicked(HeirloomId, ...)
  return OrigClicked(self, SelectedId(self) or HeirloomId, ...)
end

Log("модуль загружен")
return M
