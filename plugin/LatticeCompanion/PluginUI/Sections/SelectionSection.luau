--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local SelectionSection = {}

function SelectionSection.create(
	ui: any,
	state: any,
	position: any,
	size: any,
	onOpen: () -> (),
	onCopy: () -> ()
): Frame
	local resolved = ui:Computed(function(use)
		return use(state.selection).resourceRef ~= nil
	end)
	return Primitives.section(ui, state, "Selection", position, size, {
		Primitives.label(ui, state, {
			Text = ui:Computed(function(use)
				local selected = use(state.selection)
				return if selected.count == 0 then "No Instance selected" else "◇ " .. selected.name
			end),
			Font = Enum.Font.BuilderSansMedium,
			Position = UDim2.fromOffset(12, 37),
			Size = UDim2.new(1, -24, 0, 22),
		}),
		Primitives.label(ui, state, {
			Text = ui:Computed(function(use)
				local selected = use(state.selection)
				return if selected.className == nil then "" else selected.className
			end),
			ColorToken = "TextSecondary",
			TextSize = 11,
			Position = UDim2.fromOffset(28, 59),
			Size = UDim2.new(1, -40, 0, 18),
		}),
		Primitives.metric(
			ui,
			state,
			"Lattice",
			ui:Computed(function(use)
				return if use(state.selection).resourceRef ~= nil then "Indexed ✓" else "Unresolved"
			end),
			78
		),
		Primitives.button(ui, state, {
			Name = "OpenInLattice",
			Text = "Open in Lattice",
			Position = UDim2.fromOffset(12, 108),
			Size = UDim2.new(0.5, -16, 0, 27),
			Enabled = resolved,
			OnActivated = onOpen,
		}),
		Primitives.button(ui, state, {
			Name = "CopyReference",
			Text = "Copy rbx://",
			Position = UDim2.new(0.5, 4, 0, 108),
			Size = UDim2.new(0.5, -16, 0, 27),
			Enabled = resolved,
			OnActivated = onCopy,
		}),
	})
end

return SelectionSection
