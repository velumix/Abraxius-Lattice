--!strict

-- Compatibility name for callers that still ask for a sparkline. The actual
-- renderer is now the native Path2D GraphModule adapter.
return require(script.Parent.GraphModule)
