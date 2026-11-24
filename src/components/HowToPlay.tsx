import { Modal } from "./Modal";

interface HowToPlayProps {
  onClose: () => void;
}

export function HowToPlay({ onClose }: HowToPlayProps) {
  return (
    <Modal title="How to play" onClose={onClose} wide>
      <div className="rules-grid">
        <section>
          <h3>Win the jungle</h3>
          <p>Enter the opponent's den, capture every opposing animal, or leave the opponent with no legal move. The game is drawn on the third occurrence of the same board with the same side to move, or after 100 plies without a capture.</p>
        </section>
        <section>
          <h3>Move and capture</h3>
          <p>Select one of your animals, then choose a glowing square. Dots mark moves and rings mark captures. Animals move one square orthogonally unless Lion or Tiger makes a river jump.</p>
        </section>
        <section>
          <h3>Animal ranks</h3>
          <p>Elephant 8, Lion 7, Tiger 6, Leopard 5, Wolf 4, Dog 3, Cat 2, Rat 1. Equal or higher rank captures lower rank. An untrapped land Rat can capture Elephant; Elephant captures Rat only when that Rat is weakened in Elephant's friendly trap.</p>
        </section>
        <section>
          <h3>River rules</h3>
          <p>Only Rats swim. Water Rats capture only other Water Rats and cannot capture across the shore. Lion crosses both river spans; Tiger crosses only the two-square span. Any Rat in the path blocks a jump.</p>
        </section>
        <section>
          <h3>Traps and dens</h3>
          <p>An enemy animal standing in one of your traps has rank zero. A trapped attacker cannot capture an untrapped animal. Friendly traps preserve rank. You may never enter your own den. Entering the opposing den wins immediately.</p>
        </section>
        <section>
          <h3>Game controls</h3>
          <p>Undo returns to your previous decision point; Redo restores it. Save and Load use portable JSON files. Flip changes only the view. In AI vs AI, Undo pauses after taking back one ply. AI vs AI lets both sides play until stopped. Mute controls synthesized game sounds.</p>
        </section>
      </div>
      <button className="primary-button" type="button" onClick={onClose}>Ready to play</button>
    </Modal>
  );
}

