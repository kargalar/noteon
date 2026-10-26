use eframe::egui::{self, Button, Context, Id, InnerResponse, Rect, Response, Ui, WidgetText};

const MENU_SCOPE: &str = "persistent-menu-scope";
const MENU_BOUNDS: &str = "persistent-menu-bounds";

#[derive(Clone, Default)]
struct MenuBounds {
    pass: u64,
    rects: Vec<Rect>,
}

pub fn pointer_in_menu(context: &Context) -> bool {
    let Some(position) = context.input(|input| input.pointer.interact_pos()) else {
        return false;
    };
    if native_popup_contains(context, position) {
        return true;
    }
    context.data(|data| {
        data.get_temp::<MenuBounds>(Id::new(MENU_BOUNDS))
            .is_some_and(|bounds| {
                bounds.pass + 1 >= context.cumulative_pass_nr()
                    && bounds.rects.iter().any(|rect| rect.contains(position))
            })
    })
}

fn native_popup_contains(context: &Context, position: egui::Pos2) -> bool {
    context.memory(|memory| {
        memory
            .layer_id_at(position)
            .is_some_and(|layer| memory.is_popup_open(layer.id))
    })
}

#[derive(Clone)]
struct OpenMenu {
    root: egui::menu::MenuRoot,
    bounds: Vec<Rect>,
    include_anchor: bool,
}

#[derive(Clone, Default)]
struct MenuScope {
    id: Option<Id>,
    bounds: Vec<Rect>,
}

fn scope(context: &Context) -> MenuScope {
    context.data(|data| data.get_temp(Id::new(MENU_SCOPE)).unwrap_or_default())
}

fn set_scope(context: &Context, scope: MenuScope) {
    context.data_mut(|data| data.insert_temp(Id::new(MENU_SCOPE), scope));
}

/// Menus retain their open child until a click changes or dismisses it.
pub trait PersistentMenuUi {
    fn persistent_menu_button<R>(
        &mut self,
        title: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>>;

    fn persistent_menu_image_button<R>(
        &mut self,
        image: egui::Image<'_>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>>;
}

impl PersistentMenuUi for Ui {
    fn persistent_menu_button<R>(
        &mut self,
        title: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let nested = scope(self.ctx()).id.is_some();
        let mut button = Button::new(title);
        if nested {
            button = button.shortcut_text("›");
        }
        let response = self.add(button);
        menu_button(self, response, nested, add_contents)
    }

    fn persistent_menu_image_button<R>(
        &mut self,
        image: egui::Image<'_>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let nested = scope(self.ctx()).id.is_some();
        let response = self.add(Button::image(image));
        menu_button(self, response, nested, add_contents)
    }
}

fn menu_button<R>(
    ui: &mut Ui,
    response: Response,
    nested: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let slot = scope(ui.ctx()).id.unwrap_or(ui.id()).with("open-menu");
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<Option<OpenMenu>>(slot).flatten());
    let selected = open
        .as_ref()
        .is_some_and(|menu| menu.root.id == response.id);
    if response.clicked() {
        if selected {
            open = None;
        } else {
            let position = if nested {
                response.rect.right_top() + egui::vec2(ui.spacing().menu_spacing, 0.0)
            } else {
                response.rect.left_bottom() + egui::vec2(0.0, ui.spacing().menu_spacing)
            };
            open = Some(OpenMenu {
                root: egui::menu::MenuRoot::new(position, response.id),
                bounds: Vec::new(),
                include_anchor: true,
            });
            // A newly opened parent must not restore a stale submenu.
            ui.ctx().data_mut(|data| {
                data.remove::<Option<OpenMenu>>(response.id.with("open-menu"));
            });
        }
    }
    let inner = show_menu(ui.ctx(), &response, &mut open, add_contents);
    if inner.as_ref().is_some_and(|(_, close)| *close) && nested {
        ui.close_menu();
    }
    ui.ctx().data_mut(|data| data.insert_temp(slot, open));
    InnerResponse::new(inner.map(|(inner, _)| inner.inner), response)
}

fn show_menu<R>(
    context: &Context,
    response: &Response,
    open: &mut Option<OpenMenu>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<(InnerResponse<R>, bool)> {
    let menu = open.as_mut().filter(|menu| menu.root.id == response.id)?;
    let outside_click = context.input(|input| {
        input.pointer.any_pressed()
            && input.pointer.interact_pos().is_some_and(|position| {
                !(menu.bounds.iter().any(|rect| rect.contains(position))
                    || menu.include_anchor && response.rect.contains(position))
            })
    });
    let inside_native_popup = context
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|position| native_popup_contains(context, position));
    if outside_click && !inside_native_popup && !menu.bounds.is_empty() {
        *open = None;
        return None;
    }

    let mut parent_scope = scope(context);
    set_scope(
        context,
        MenuScope {
            id: Some(response.id),
            bounds: Vec::new(),
        },
    );
    // Capture the contents directly: egui 0.31's MenuRoot::show only returns
    // its InnerResponse when the menu closes.
    let mut inner = None;
    let (action, _) = menu.root.show(response, |ui| {
        let result = add_contents(ui);
        inner = Some(InnerResponse::new(result, ui.response()));
    });
    if let Some(inner) = &mut inner {
        inner.response.rect = menu.root.menu_state.read().rect;
    }
    let mut bounds = scope(context).bounds;
    if let Some(inner) = &inner {
        bounds.push(inner.response.rect);
    }
    let pass = context.cumulative_pass_nr();
    context.data_mut(|data| {
        let registry = data.get_temp_mut_or_default::<MenuBounds>(Id::new(MENU_BOUNDS));
        if registry.pass != pass {
            registry.pass = pass;
            registry.rects.clear();
        }
        registry.rects.extend(bounds.iter().copied());
    });
    menu.bounds.clone_from(&bounds);
    if parent_scope.id.is_some() {
        parent_scope.bounds.extend(bounds);
    }
    set_scope(context, parent_scope);
    let close = action.is_close();
    if close {
        *open = None;
    }
    inner.map(|inner| (inner, close))
}

pub trait PersistentContextMenu {
    fn persistent_context_menu(
        &self,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Option<InnerResponse<()>>;
}

impl PersistentContextMenu for Response {
    fn persistent_context_menu(
        &self,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Option<InnerResponse<()>> {
        let slot = Id::new("persistent-context-menu");
        let mut open = self
            .ctx
            .data(|data| data.get_temp::<Option<OpenMenu>>(slot).flatten());
        let marker = self.id.with("__context_menu");
        let requested = self.ctx.memory(|memory| memory.is_popup_open(marker));
        if self.interact(egui::Sense::click()).secondary_clicked() || requested {
            let position = self
                .interact_pointer_pos()
                .unwrap_or_else(|| self.rect.left_bottom());
            open = Some(OpenMenu {
                root: egui::menu::MenuRoot::new(position, self.id),
                bounds: Vec::new(),
                include_anchor: false,
            });
            self.ctx.data_mut(|data| {
                data.remove::<Option<OpenMenu>>(self.id.with("open-menu"));
            });
            if requested {
                self.ctx.memory_mut(|memory| memory.close_popup());
            }
        }
        let inner = show_menu(&self.ctx, self, &mut open, add_contents);
        self.ctx.data_mut(|data| data.insert_temp(slot, open));
        inner.map(|(inner, _)| inner)
    }
}

pub fn popup_below_widget<R>(
    ui: &mut Ui,
    popup_id: Id,
    response: &Response,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let slot = popup_id.with("persistent-popup");
    if !ui.memory(|memory| memory.is_popup_open(popup_id)) {
        ui.ctx()
            .data_mut(|data| data.remove::<Option<OpenMenu>>(slot));
        return None;
    }
    let mut open = ui
        .ctx()
        .data(|data| data.get_temp::<Option<OpenMenu>>(slot).flatten());
    if open.is_none() {
        open = Some(OpenMenu {
            root: egui::menu::MenuRoot::new(response.rect.left_bottom(), response.id),
            bounds: Vec::new(),
            include_anchor: true,
        });
        ui.ctx()
            .data_mut(|data| data.remove::<Option<OpenMenu>>(response.id.with("open-menu")));
    }
    let inner = show_menu(ui.ctx(), response, &mut open, add_contents);
    if open.is_none() {
        ui.memory_mut(|memory| memory.close_popup());
    }
    ui.ctx().data_mut(|data| data.insert_temp(slot, open));
    inner.map(|(inner, _)| inner)
}
