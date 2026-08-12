--!strict

-- Every plugin color originates here. Studio theme colors are primary; compact
-- semantic accents are derived once and never scattered through components.

local Tokens = {}

local function studioColor(theme: StudioTheme, guide: Enum.StudioStyleGuideColor): Color3
	return theme:GetColor(guide, Enum.StudioStyleGuideModifier.Default)
end

function Tokens.fromStudio(theme: StudioTheme): { [string]: Color3 }
	local background = studioColor(theme, Enum.StudioStyleGuideColor.MainBackground)
	local textPrimary = studioColor(theme, Enum.StudioStyleGuideColor.MainText)

	-- MainButton is an action colour in some Studio themes (including the
	-- current Vinegar build) and can resolve to saturated blue. Cockpit surfaces
	-- must remain neutral, so derive elevation from Studio's background and text
	-- direction instead. This preserves dark/light behaviour without fixed RGB.
	local surface = background:Lerp(textPrimary, 0.025)
	local surfaceRaised = background:Lerp(textPrimary, 0.055)
	local hover = background:Lerp(textPrimary, 0.09)
	return table.freeze({
		Background = background,
		Surface = surface,
		SurfaceRaised = surfaceRaised,
		SurfaceHover = hover,
		Border = studioColor(theme, Enum.StudioStyleGuideColor.Border),
		BorderStrong = studioColor(theme, Enum.StudioStyleGuideColor.ButtonBorder),
		TextPrimary = textPrimary,
		TextSecondary = studioColor(theme, Enum.StudioStyleGuideColor.SubText),
		TextMuted = studioColor(theme, Enum.StudioStyleGuideColor.DimmedText),
		Accent = studioColor(theme, Enum.StudioStyleGuideColor.LinkText),
		Success = studioColor(theme, Enum.StudioStyleGuideColor.CheckedFieldIndicator),
		Warning = studioColor(theme, Enum.StudioStyleGuideColor.WarningText),
		Danger = studioColor(theme, Enum.StudioStyleGuideColor.ErrorText),
		Recording = studioColor(theme, Enum.StudioStyleGuideColor.ErrorText),
		Focus = studioColor(theme, Enum.StudioStyleGuideColor.InputFieldBorder),
	})
end

return Tokens
