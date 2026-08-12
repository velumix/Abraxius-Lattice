export type Item = {
    id: string,
    quantity: number,
}

local Inventory = {}

function Inventory.has(items: { Item }, itemId: string): boolean
    for _, item in items do
        if item.id == itemId and item.quantity > 0 then
            return true
        end
    end
    return false
end

return Inventory

