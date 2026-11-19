-- Trial class 400201: a ninth slot in the character creation window.
-- The game's original module lies next to it as WBP_CreateRoleInfoUi_Orig.lua.
--
-- The BP function InitSubItemUi collects CareerItems from CareerWrap.GetAllChildren and sets WBP = self on each.
-- It is called by BP_Construct, i.e. the original Construct. So we add the widget BEFORE it,
-- the rest of the window picks up the new slot by itself.
local M = require("Game.System.CreateRole.WBP_CreateRoleInfoUi_Orig")

local ITEM_CLASS = "/Game/TGF/UI/WBP/CreateRole/Info/WBP_CreateRoleCareerItem.WBP_CreateRoleCareerItem_C"
local WANT_SLOTS = 9
local LOG_PATH = "<data>/new_class/logs/lua_create_role.log"

local function Log(Msg)
  pcall(function()
    local F = io.open(LOG_PATH, "a")
    if F then
      F:write(os.date("%H:%M:%S ") .. tostring(Msg) .. "\n")
      F:close()
    end
  end)
end

local function CopySlotLayout(Slot, Sample)
  Slot:SetPadding(Sample.Padding)
  Slot:SetHorizontalAlignment(Sample.HorizontalAlignment)
  Slot:SetVerticalAlignment(Sample.VerticalAlignment)
end

local function AddSlots(self)
  local Wrap = self.CareerWrap
  if not Wrap then
    Log("no CareerWrap")
    return
  end
  local Have = Wrap:GetChildrenCount()
  if Have >= WANT_SLOTS then
    return
  end
  local Class = UE.UClass.Load(ITEM_CLASS)
  local Sample = Wrap:GetChildAt(0)
  for _ = Have + 1, WANT_SLOTS do
    local Item = UE.UWidgetBlueprintLibrary.Create(self, Class)
    local Slot = Wrap:AddChild(Item)
    if Slot and Sample and Sample.Slot then
      local Ok, Err = pcall(CopySlotLayout, Slot, Sample.Slot)
      if not Ok then
        Log("slot layout: " .. tostring(Err))
      end
    end
  end
  Log("slots in the window: " .. tostring(Have) .. " -> " .. tostring(Wrap:GetChildrenCount()))
end

local OrigConstruct = M.Construct

function M:Construct()
  -- An error while adding the slot must not break the creation window.
  local Ok, Err = pcall(AddSlots, self)
  if not Ok then
    Log("AddSlots: " .. tostring(Err))
  end
  return OrigConstruct(self)
end

Log("module loaded")
return M
