-- Trial class 400201: the cross on equipment.
-- The original module lies next to it as CommonFunction_Orig.lua.
--
-- BP_Common_ItemEntry and ItemTooltipUtils: for the order's career outside the order inventory only consumables are "wearable",
-- everything else gets a cross. There are no other IsMithrilCareer calls in the game. With false both functions go to the regular
-- check UBattleUtility.MatchCareer against the UeSource_Equipment_SubType table. The same one is used in the order inventory too.
local M = require("Base.CommonFunction_Orig")

function M.IsMithrilCareer(_)
  return false
end

return M
