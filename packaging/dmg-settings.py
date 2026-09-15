# dmgbuild 配置
# This file is executed by dmgbuild with the source directory as `source_folder`.

import os

# The arrow is part of the fixed 640x420 background and sits between the icons.
background = os.path.join(defines['root_dir'], 'packaging', 'dmg-background.png')
hide = ['.background.png']

# The source staging directory is passed through dmgbuild's defines.
source_folder = defines['source_folder']

files = [source_folder + '/GoldTicker.app']
symlinks = {'Applications': '/Applications'}
format = 'UDZO'

window_rect = ((100, 100), (640, 420))
icon_size = 112
text_size = 14
icon_locations = {
    'GoldTicker.app': (160, 160),
    'Applications': (480, 160),
}

show_status_bar = True
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_view_options = False
arrange_by = None

application_bundle = 'GoldTicker.app'
