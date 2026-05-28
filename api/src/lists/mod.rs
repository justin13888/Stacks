pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;

pub fn router() -> Router {
    Router::new()
        .push(
            Router::with_path("users/me/lists")
                .get(handlers::list_lists)
                .post(handlers::create_list),
        )
        .push(
            Router::with_path("users/me/lists/<id>")
                .get(handlers::get_list)
                .put(handlers::update_list)
                .delete(handlers::delete_list),
        )
        .push(
            Router::with_path("users/me/lists/<id>/items")
                .get(handlers::get_list_items)
                .post(handlers::add_item),
        )
        .push(
            Router::with_path("users/me/lists/<id>/items/<cameo_id>").delete(handlers::remove_item),
        )
        .push(Router::with_path("users/me/lists/<id>/items/reorder").put(handlers::reorder_items))
}
