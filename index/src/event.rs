use std::collections::HashSet;
use yew_agent::{Agent, AgentLink, Context, HandlerId};

use common::socket::ClientCall;

pub struct Bus {
    link: AgentLink<Bus>,
    subscribers: HashSet<HandlerId>,
}

impl Agent for Bus {
    type Reach = Context<Self>;
    type Message = ();
    type Input = ClientCall;
    type Output = ClientCall;

    fn create(link: AgentLink<Self>) -> Self {
        Self {
            link,
            subscribers: HashSet::new(),
        }
    }

    fn update(&mut self, _msg: Self::Message) {}

    fn handle_input(&mut self, message: Self::Input, _id: HandlerId) {
        for sub in &self.subscribers {
            self.link.respond(*sub, message.clone());
        }
    }

    fn connected(&mut self, id: HandlerId) {
        self.subscribers.insert(id);
    }

    fn disconnected(&mut self, id: HandlerId) {
        self.subscribers.remove(&id);
    }
}
