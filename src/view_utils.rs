use crate::blackboard::Blackboard;
use crate::messages::Message;
use iced::widget::{
    button, column, container, horizontal_space, row, text, vertical_space, Column, Container,
};
use iced::{Center, Element};
use iced_futures::core::Length::Fill;
use self_update::cargo_crate_version;

pub fn centered_container(content: Element<Message>) -> Element<Message> {
    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}
pub fn container_with_title(title: String, content: Column<Message>) -> Element<Message> {
    let col = column![]
        .push(vertical_space().height(40))
        .push(text(title).size(70))
        .align_x(Center)
        .push(vertical_space().height(50))
        .spacing(0)
        .push(content.align_x(Center));

    let r = row![]
        .push(horizontal_space())
        .push(col)
        .push(horizontal_space());

    Container::new(r)
        .center(Fill)
        .width(Fill)
        .height(Fill)
        .into()
}

pub fn container_with_top_bar_and_side_view<'a, 'b>(
    content: Container<'a, Message>,
    blackboard: &'b Blackboard,
) -> Element<'a, Message> {
    let config = &blackboard.config;
    let header = container(
        row![
            text(format!("Logged in as  {}", config.get_username())),
            horizontal_space(),
            column!["drops", cargo_crate_version!()],
            horizontal_space(),
            button(text("logout").center()).on_press(Message::Logout)
        ]
        .padding(10)
        .align_y(Center),
    );

    let games = config.get_account_games();
    let games: Element<Message> = column(games.into_iter().map(|x| {
        button(text(x.name.to_string()).center())
            .width(Fill)
            .on_press(Message::SelectGame(x.clone()))
            .into()
    }))
    .spacing(10)
    .into();

    let sidebar_column = column![
        row![
            horizontal_space(),
            text("Games").align_x(Center).size(22),
            horizontal_space()
        ],
        vertical_space().height(15),
        games,
        vertical_space()
    ]
    .padding(10)
    .width(160);

    let sidebar = container(sidebar_column)
        .style(container::dark)
        .center_y(Fill);

    column![
        header,
        row![
            sidebar,
            container(content.align_x(Center))
                .width(Fill)
                .height(Fill)
                .padding(10)
        ]
    ]
    .into()
}
